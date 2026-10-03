//! Bounded, mono device capture. Audio never touches the conversation journal.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use rodio::Source;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
};

pub(super) const RATE: usize = 16_000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Device {
    pub id: String,
    pub name: String,
}

pub(super) fn devices(input: bool) -> Result<Vec<Device>, String> {
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|_| "Não foi possível listar os dispositivos de áudio.".to_owned())?;
    Ok(devices
        .filter_map(|device| {
            Some(Device {
                id: device.id().ok()?.to_string(),
                name: device.description().ok()?.name().to_owned(),
            })
        })
        .collect())
}

pub(super) fn device(id: Option<&str>, input: bool) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if let Some(id) = id {
        let devices = if input {
            host.input_devices()
        } else {
            host.output_devices()
        }
        .map_err(|_| "Dispositivo de áudio indisponível.".to_owned())?;
        return devices.into_iter().find(|d| d.id().is_ok_and(|value| value.to_string() == id))
            .ok_or_else(|| "O dispositivo escolhido foi desconectado. Selecione outro nas configurações de voz.".into());
    }
    (if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    })
    .ok_or_else(|| "Nenhum dispositivo de áudio está disponível.".into())
}

pub(super) struct Capture {
    pub _stream: cpal::Stream,
    pub audio: CaptureAudio,
    pub failed: Arc<AtomicBool>,
    pub dropped: Arc<AtomicBool>,
    pub rate: u32,
}

/// Bound queued microphone audio by duration, independent of the device callback size.
pub(super) struct CaptureAudio {
    receiver: mpsc::Receiver<Vec<f32>>,
    queued: Arc<AtomicUsize>,
}
struct CaptureSender {
    sender: mpsc::SyncSender<Vec<f32>>,
    queued: Arc<AtomicUsize>,
    limit: usize,
    dropped: Arc<AtomicBool>,
}
impl CaptureAudio {
    fn channel(rate: u32) -> (CaptureSender, Self) {
        let (sender, receiver) = mpsc::sync_channel(4096);
        let queued = Arc::new(AtomicUsize::new(0));
        (
            CaptureSender {
                sender,
                queued: queued.clone(),
                limit: rate as usize * 10,
                dropped: Arc::new(AtomicBool::new(false)),
            },
            Self { receiver, queued },
        )
    }
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Vec<f32>, mpsc::RecvTimeoutError> {
        let samples = self.receiver.recv_timeout(timeout)?;
        self.queued.fetch_sub(samples.len(), Ordering::AcqRel);
        Ok(samples)
    }
    pub fn try_iter(&self) -> impl Iterator<Item = Vec<f32>> + '_ {
        std::iter::from_fn(|| {
            let samples = self.receiver.try_recv().ok()?;
            self.queued.fetch_sub(samples.len(), Ordering::AcqRel);
            Some(samples)
        })
    }
}
impl CaptureSender {
    fn send(&self, samples: Vec<f32>) {
        if samples.is_empty() {
            return;
        }
        let size = samples.len();
        if self
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                queued.checked_add(size).filter(|size| *size <= self.limit)
            })
            .is_err()
        {
            self.dropped.store(true, Ordering::Release);
        } else if self.sender.try_send(samples).is_err() {
            self.queued.fetch_sub(size, Ordering::AcqRel);
            self.dropped.store(true, Ordering::Release);
        }
    }
}

pub(super) fn capture(id: Option<&str>, enabled: Arc<AtomicBool>) -> Result<Capture, String> {
    let device = device(id, true)?;
    let supported = device
        .default_input_config()
        .map_err(|_| "O microfone não oferece um formato compatível.".to_owned())?;
    let config: cpal::StreamConfig = supported.clone().into();
    let (sender, audio) = CaptureAudio::channel(config.sample_rate);
    let failed = Arc::new(AtomicBool::new(false));
    let dropped = sender.dropped.clone();
    let error = failed.clone();
    let error_callback = move |_| {
        error.store(true, Ordering::Release);
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => input::<f32>(&device, &config, sender, enabled, error_callback),
        cpal::SampleFormat::I16 => input::<i16>(&device, &config, sender, enabled, error_callback),
        cpal::SampleFormat::U16 => input::<u16>(&device, &config, sender, enabled, error_callback),
        cpal::SampleFormat::I32 => input::<i32>(&device, &config, sender, enabled, error_callback),
        _ => return Err("O formato do microfone não é compatível. Selecione outro dispositivo.".into()),
    }.map_err(|_| "Não foi possível abrir o microfone. Verifique a permissão de microfone do Jarvis nas configurações do sistema.".to_owned())?;
    stream
        .play()
        .map_err(|_| "Não foi possível iniciar o microfone.".to_owned())?;
    Ok(Capture {
        _stream: stream,
        audio,
        failed,
        dropped,
        rate: config.sample_rate,
    })
}

fn input<T: cpal::SizedSample + cpal::Sample>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: CaptureSender,
    enabled: Arc<AtomicBool>,
    error: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels);
    device.build_input_stream(
        config,
        move |samples: &[T], _| {
            if !enabled.load(Ordering::Acquire) {
                return;
            }
            let mono = samples
                .chunks_exact(channels)
                .map(|frame| {
                    frame
                        .iter()
                        .map(|sample| {
                            let value = f32::from_sample(*sample);
                            if value.is_finite() {
                                value.clamp(-1., 1.)
                            } else {
                                0.
                            }
                        })
                        .sum::<f32>()
                        / channels as f32
                })
                .collect();
            sender.send(mono);
        },
        error,
        None,
    )
}

/// Keep the fractional device position between callbacks; never stretch a chunk independently.
pub(super) struct Resampler {
    samples: VecDeque<f32>,
    position: f64,
    ratio: f64,
}
impl Resampler {
    pub fn new(rate: u32) -> Self {
        Self {
            samples: VecDeque::new(),
            position: 0.,
            ratio: f64::from(rate) / RATE as f64,
        }
    }
    pub fn push(&mut self, samples: Vec<f32>) -> Vec<f32> {
        self.samples.extend(samples);
        let mut output = Vec::new();
        while self.position + 1. < self.samples.len() as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            output.push(self.samples[index] * (1. - fraction) + self.samples[index + 1] * fraction);
            self.position += self.ratio;
        }
        let consumed = (self.position.floor() as usize).min(self.samples.len());
        self.samples.drain(..consumed);
        self.position -= consumed as f64;
        output
    }
}

pub(super) fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.;
    }
    (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Meter the samples actually consumed by playback, rather than a synthetic talking loop.
pub(super) struct Metered<S> {
    source: S,
    meter: Arc<std::sync::atomic::AtomicU32>,
    sum: f32,
    count: u32,
    window: u32,
}
impl<S: Source> Metered<S> {
    pub fn new(source: S, meter: Arc<std::sync::atomic::AtomicU32>) -> Self {
        let window = source.sample_rate().get() * u32::from(source.channels().get()) / 25;
        Self {
            source,
            meter,
            sum: 0.,
            count: 0,
            window: window.max(1),
        }
    }
}
impl<S: Source> Iterator for Metered<S> {
    type Item = rodio::Sample;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.source.next()?;
        self.sum += value * value;
        self.count += 1;
        if self.count >= self.window {
            self.meter.store(
                ((self.sum / self.count as f32).sqrt() * 4.)
                    .min(1.)
                    .to_bits(),
                Ordering::Release,
            );
            self.count = 0;
            self.sum = 0.;
        }
        Some(value)
    }
}
impl<S: Source> Source for Metered<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.source.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.source.total_duration()
    }
}

/// Pre-roll protects initial consonants. Silence ends a phrase, never the dictation.
pub(super) struct Utterance {
    pre_roll: VecDeque<f32>,
    pub samples: Vec<f32>,
    silence: usize,
    speech: usize,
    silence_samples: usize,
}
impl Utterance {
    pub fn new(silence_ms: u32) -> Self {
        Self {
            pre_roll: VecDeque::new(),
            samples: Vec::new(),
            silence: 0,
            speech: 0,
            silence_samples: silence_ms as usize * RATE / 1000,
        }
    }
    pub fn push(&mut self, samples: &[f32], speaking: bool) -> Option<Vec<f32>> {
        if speaking {
            if self.samples.is_empty() {
                self.samples.extend(self.pre_roll.drain(..));
            }
            self.speech += samples.len();
            self.silence = 0;
        } else {
            self.silence += samples.len();
        }
        if self.samples.is_empty() && !speaking {
            self.pre_roll.extend(samples);
            while self.pre_roll.len() > RATE / 4 {
                self.pre_roll.pop_front();
            }
            return None;
        }
        self.samples.extend(samples);
        if self.silence >= self.silence_samples || self.samples.len() >= RATE * 90 {
            return self.finish();
        }
        None
    }
    pub fn finish(&mut self) -> Option<Vec<f32>> {
        let samples = std::mem::take(&mut self.samples);
        let accepted = self.speech >= RATE / 5;
        self.silence = 0;
        self.speech = 0;
        self.pre_roll.clear();
        accepted.then_some(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_keeps_initial_consonants_and_waits_for_a_pause() {
        let mut utterance = Utterance::new(650);
        assert!(utterance.push(&vec![0.; 4000], false).is_none());
        assert!(utterance.push(&vec![0.2; 6000], true).is_none());
        assert!(utterance.push(&vec![0.; 10000], false).is_none());
        let samples = utterance.push(&vec![0.; 500], false).unwrap();
        assert_eq!(samples.len(), 20500);
        assert_eq!(samples[4000], 0.2);
        assert!(utterance.push(&vec![0.; 16000], false).is_none());
    }
    #[test]
    fn noise_and_cancelled_audio_do_not_become_messages() {
        let mut utterance = Utterance::new(650);
        utterance.push(&[0.4; 256], true);
        assert!(utterance.finish().is_none());
        utterance.push(&[0.4; 6000], true);
        // Cancellation drops capture and recognition instead of reusing audio.
        drop(utterance);
        let mut utterance = Utterance::new(650);
        assert!(utterance.finish().is_none());
    }
    #[test]
    fn dictation_delivers_each_pause_and_keeps_the_last_spoken_tail() {
        let mut utterance = Utterance::new(650);
        assert!(utterance.push(&vec![0.2; 6000], true).is_none());
        let first = utterance.push(&vec![0.; 10400], false).unwrap();
        assert_eq!(first.len(), 16400);
        assert!(utterance.push(&vec![0.; 8000], false).is_none());
        assert!(utterance.push(&vec![0.3; 6400], true).is_none());
        let last = utterance.finish().unwrap();
        assert_eq!(last.len(), 10400);
        assert_eq!(last[4000], 0.3);
        assert!(utterance.finish().is_none());
    }
    #[test]
    fn split_device_callbacks_have_the_same_clock_as_a_single_buffer() {
        let source: Vec<f32> = (0..48000).map(|i| (i as f32 / 80.).sin()).collect();
        let expected = Resampler::new(48000).push(source.clone());
        let mut resampler = Resampler::new(48000);
        let actual: Vec<f32> = source
            .chunks(333)
            .flat_map(|chunk| resampler.push(chunk.to_vec()))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), RATE);
    }
    #[test]
    fn next_phrase_is_queued_during_transcription_with_a_fixed_audio_budget() {
        let (sender, queue) = CaptureAudio::channel(16_000);
        // More than twelve small callbacks must survive a local decode.
        for _ in 0..100 {
            sender.send(vec![0.25; 512]);
        }
        assert!(!sender.dropped.load(Ordering::Acquire));
        assert_eq!(
            queue.try_iter().map(|samples| samples.len()).sum::<usize>(),
            51200
        );
        assert_eq!(queue.queued.load(Ordering::Acquire), 0);
        sender.send(vec![0.; RATE * 10]);
        sender.send(vec![0.5; 1]);
        assert!(sender.dropped.load(Ordering::Acquire));
        assert_eq!(
            queue.recv_timeout(std::time::Duration::ZERO).unwrap().len(),
            RATE * 10
        );
        assert!(queue.try_iter().next().is_none());
        assert_eq!(queue.queued.load(Ordering::Acquire), 0);
    }
}
