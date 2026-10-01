"""Observable offline runner contracts, without model downloads."""

from array import array
from contextlib import redirect_stdout
import importlib.util
import io
import json
import logging
import math
import os
from pathlib import Path
import tempfile
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import wave


sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("audio_runner", Path(__file__).with_name("runner.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class AudioRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def request(self, **extra):
        return {"action": "narrate", "text": "Olá, Jarvis.", "models": str(self.root), "output": str(self.root / "voice.wav"), **extra}

    def test_accepts_explicit_pt_br_voices_and_speed(self):
        request = self.request(voice="pm_alex", speed=1.2)
        self.assertEqual(runner.validate(request), request)

    def test_rejects_non_finite_duration_and_invalid_seeds(self):
        for duration in (float("nan"), float("inf"), -2, True):
            with self.subTest(duration=duration), self.assertRaises(runner.AudioError):
                runner.validate(self.request(action="music", prompt="Trilha calma", duration=duration))
        for seed in (-1, 2**32, True, 1.2):
            with self.subTest(seed=seed), self.assertRaises(runner.AudioError):
                runner.validate(self.request(action="music", prompt="Trilha calma", duration=4, seed=seed))

    def test_rejects_missing_models_relative_outputs_and_unknown_voice(self):
        for change in ({"models": str(self.root / "missing")}, {"output": "voice.wav"}, {"voice": "am_adam"}):
            with self.subTest(change=change), self.assertRaises(runner.AudioError):
                runner.validate(self.request(**change))

    def test_writes_audible_mono_pcm_with_actual_duration(self):
        output = self.root / "voice.wav"
        samples = array("f", [0.0, 0.5, -0.5, 0.25] * 100)
        result = runner.write_wav(output, samples, 100)
        self.assertEqual(result["duration"], 4)
        with wave.open(str(output), "rb") as wav:
            self.assertEqual((wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()), (1, 2, 100, 400))
            self.assertNotEqual(set(wav.readframes(400)), {0})

    def test_never_overwrites_existing_output(self):
        output = self.root / "voice.wav"
        output.write_bytes(b"user artifact")
        with self.assertRaises(runner.AudioError):
            runner.write_wav(output, array("f", [0.5]), 24_000)
        self.assertEqual(output.read_bytes(), b"user artifact")

    def test_accepts_empty_staged_file_owned_by_rust_host(self):
        output = self.root / "voice.wav"
        output.touch()
        runner.validate(self.request())
        runner.write_wav(output, array("f", [0.5] * 10), 100)
        with wave.open(str(output), "rb") as wav:
            self.assertEqual(wav.getnframes(), 10)

    @unittest.skipIf(__import__("os").name == "nt", "Symlink permissions differ on Windows")
    def test_rejects_output_symlink_even_when_target_is_empty(self):
        target = self.root / "target.wav"
        target.touch()
        output = self.root / "voice.wav"
        output.symlink_to(target)
        with self.assertRaises(runner.AudioError):
            runner.validate(self.request())
        with self.assertRaises(runner.AudioError):
            runner.write_wav(output, array("f", [0.5]), 100)
        self.assertEqual(target.read_bytes(), b"")

    def test_rejects_silent_non_finite_or_empty_audio(self):
        for samples in (array("f"), array("f", [0] * 4), array("f", [float("nan")])):
            with self.subTest(samples=samples), self.assertRaises(runner.AudioError):
                runner.write_wav(self.root / "voice.wav", samples, 24_000)
        self.assertFalse((self.root / "voice.wav").exists())

    def test_crossfade_loop_fits_requested_timeline_and_fades_edges(self):
        seed = array("f", (math.sin(index / 5) * 0.4 for index in range(100)))
        output = runner.loop_background(seed, 100, 3.5)
        self.assertEqual(len(output), 350)
        self.assertEqual((output[0], output[-1]), (0, 0))
        self.assertTrue(all(math.isfinite(value) for value in output))
        self.assertTrue(any(abs(value) > 0.2 for value in output))

    def test_narration_respiros_preserve_every_speech_sample_and_extend_wav_duration(self):
        speech = array("f", [0.0002, 0.4, -0.4, -0.0002] * 100)
        samples = runner.narration_samples(speech, 1000)
        self.assertEqual(samples[:250], array("f", [0] * 250))
        self.assertEqual(samples[250:-200], speech)
        self.assertEqual(samples[-200:], array("f", [0] * 200))
        output = self.root / "voice.wav"
        result = runner.write_wav(output, samples, 1000)
        self.assertEqual(result["duration"], 0.85)
        with wave.open(str(output), "rb") as wav:
            pcm = array("h", wav.readframes(wav.getnframes()))
        self.assertEqual(pcm[:250], array("h", [0] * 250))
        self.assertEqual(pcm[-200:], array("h", [0] * 200))
        self.assertEqual(pcm[250:-200], array("h", (round(value * 32767) for value in speech)))

    def test_narration_keeps_natural_phonemes_and_punctuation_pauses(self):
        model_root = self.root / "kokoro"
        model_root.mkdir()
        for name in ("kokoro-v1.0.onnx", "voices-v1.0.bin"):
            (model_root / name).touch()
        kokoro = SimpleNamespace(get_voices=lambda: ["pm_alex"])
        speech = [0.0002, 0.4, -0.4, -0.0002]
        kokoro.create = Mock(return_value=(SimpleNamespace(tolist=lambda: speech), 1000))
        modules = {
            "espeakng_loader": SimpleNamespace(get_library_path=lambda: "managed-espeak"),
            "onnxruntime": SimpleNamespace(SessionOptions=SimpleNamespace, InferenceSession=lambda *args, **kwargs: object()),
            "kokoro_onnx": SimpleNamespace(Kokoro=SimpleNamespace(from_session=lambda *args: kokoro)),
        }
        with patch.dict(sys.modules, modules), redirect_stdout(io.StringIO()):
            result = runner.narrate(self.request(text="  Primeira frase. Segunda frase.  "))
        kokoro.create.assert_called_once_with(
            "Primeira frase. Segunda frase.", voice="pm_alex", speed=1, lang="pt-br",
            trim=False, sentence_pause=0.35, clause_pause=0.15,
        )
        self.assertEqual(result["narrationTiming"], {"leadIn": 0.25, "tailOut": 0.2})
        self.assertEqual(result["frames"], 454)

    def test_auto_music_device_stays_cpu_without_cuda_even_with_mps(self):
        torch = SimpleNamespace(cuda=SimpleNamespace(is_available=lambda: False), backends=SimpleNamespace(mps=SimpleNamespace(is_available=lambda: True)))
        self.assertEqual(runner.music_device(torch, "auto"), "cpu")
        self.assertEqual(runner.music_device(torch, "mps"), "mps")
        with self.assertRaises(runner.AudioError):
            runner.music_device(torch, "cuda")

    def test_model_config_notice_filter_preserves_actionable_warnings(self):
        logger = logging.getLogger("jarvis_audio_warning_test")
        stream = io.StringIO()
        handler = logging.StreamHandler(stream)
        handler.addFilter(runner.model_warning_filter)
        logger.addHandler(handler)
        try:
            logger.warning("Config of the decoder is overwritten by shared decoder config: expected metadata")
            logger.warning("Model weights are missing or incompatible")
            self.assertNotIn("expected metadata", stream.getvalue())
            self.assertIn("missing or incompatible", stream.getvalue())
        finally:
            logger.removeHandler(handler)

    def test_invalid_request_produces_structured_error_without_inference(self):
        stream = io.StringIO()
        with redirect_stdout(stream), patch.object(runner, "narrate") as narrate:
            status = runner.main(["--request", '{"action":"narrate","text":""}'])
        self.assertEqual(status, 1)
        self.assertEqual(json.loads(stream.getvalue())["error"]["code"], "invalid_request")
        narrate.assert_not_called()

    def test_success_keeps_final_result_parseable_after_progress(self):
        def narrate(_request):
            runner.progress("narrating")
            return {"engine": "kokoro-onnx", "duration": 2, "output": str(self.root / "voice.wav")}
        stream = io.StringIO()
        with redirect_stdout(stream), patch.object(runner, "narrate", side_effect=narrate):
            status = runner.main(["--request", json.dumps(self.request())])
        self.assertEqual(status, 0)
        lines = [json.loads(line) for line in stream.getvalue().splitlines()]
        self.assertEqual(lines[0]["event"], "progress")
        self.assertEqual(lines[-1]["engine"], "kokoro-onnx")

    def test_concurrent_job_reports_queue_and_resumes_after_lock_release(self):
        models = self.root / "models"
        models.mkdir()
        code = (
            "import importlib.util,pathlib,sys; sys.dont_write_bytecode=True; "
            f"s=importlib.util.spec_from_file_location('runner',{str(Path(__file__).with_name('runner.py'))!r}); "
            "m=importlib.util.module_from_spec(s);s.loader.exec_module(m); "
            f"lock=m.inference_lock(pathlib.Path({str(models)!r})); "
            "lock.__enter__(); print('acquired',flush=True); lock.__exit__(None,None,None)"
        )
        process = None
        try:
            # Both Core versions share the host-owned lock, despite different
            # model locations. The child simulates the newly installed version.
            old_models = self.root / "previous-models"
            old_models.mkdir()
            with patch.dict(os.environ, {"JARVIS_AUDIO_LOCK_PATH": str(self.root / "shared.lock")}):
                with runner.inference_lock(old_models):
                    process = subprocess.Popen([sys.executable, "-c", code], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    with self.assertRaises(subprocess.TimeoutExpired) as timeout:
                        process.communicate(timeout=1)
                    self.assertIn(b'"phase": "queued"', timeout.exception.output or b"")
            stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 0, stderr.decode())
            self.assertIn(b"acquired", stdout)
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main()
