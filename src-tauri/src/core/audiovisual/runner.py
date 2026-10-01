"""Offline audio inference in a private, turn-owned Python subprocess.

The Rust host validates project paths, stages the output and publishes it only
after validating the resulting WAV. Models must already be provisioned by Core.
"""

from __future__ import annotations

import argparse
from array import array
from contextlib import contextmanager
import json
import logging
import math
import os
from pathlib import Path
import sys
import stat
import wave
import warnings


VOICES = ("pf_dora", "pm_alex", "pm_santa")
DEVICES = ("auto", "cpu", "cuda", "mps")


class AudioError(Exception):
    """A recoverable error returned to the tool caller."""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def progress(phase: str) -> None:
    print(json.dumps({"event": "progress", "phase": phase}), flush=True)


def number(value: object, name: str, minimum: float, maximum: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise AudioError("invalid_request", f"{name} precisa ser um número.")
    value = float(value)
    if not math.isfinite(value) or not minimum <= value <= maximum:
        raise AudioError("invalid_request", f"{name} precisa estar entre {minimum} e {maximum}.")
    return value


def validate(request: object) -> dict:
    if not isinstance(request, dict):
        raise AudioError("invalid_request", "A solicitação de áudio precisa ser um objeto JSON.")
    action = request.get("action")
    if action not in ("narrate", "music"):
        raise AudioError("invalid_request", "Escolha narração ou música para gerar o áudio.")
    text_key = "text" if action == "narrate" else "prompt"
    text = request.get(text_key)
    if not isinstance(text, str) or not text.strip() or len(text) > 50_000:
        raise AudioError("invalid_request", f"{text_key} precisa conter de 1 a 50000 caracteres.")
    for key in ("output", "models"):
        value = request.get(key)
        if not isinstance(value, str) or not Path(value).is_absolute():
            raise AudioError("invalid_request", f"{key} precisa ser um caminho absoluto.")
    output = Path(request["output"])
    if output.suffix.lower() != ".wav" or not output.parent.is_dir():
        raise AudioError("invalid_request", "O destino precisa ser um arquivo WAV em uma pasta existente.")
    if output.is_symlink() or (output.exists() and (not output.is_file() or output.stat().st_size != 0)):
        raise AudioError("output_exists", "A saída temporária já está preenchida; escolha outro destino.")
    if not Path(request["models"]).is_dir():
        raise AudioError("models_missing", "Os modelos locais estão indisponíveis. Repare o componente Audiovisual no Core.")
    if request.get("device", "auto") not in DEVICES:
        raise AudioError("invalid_request", "O dispositivo precisa ser auto, cpu, cuda ou mps.")
    if action == "narrate":
        if request.get("voice", "pm_alex") not in VOICES:
            raise AudioError("invalid_request", "Escolha uma das vozes PT-BR disponíveis.")
        number(request.get("speed", 1), "speed", 0.5, 2)
    else:
        number(request.get("duration"), "duration", 0.1, 600)
        seed = request.get("seed", 0)
        if isinstance(seed, bool) or not isinstance(seed, int) or not 0 <= seed < 2**32:
            raise AudioError("invalid_request", "A semente precisa ser um inteiro de 0 a 4294967295.")
    return request


def require_file(path: Path) -> Path:
    if not path.is_file():
        raise AudioError("models_missing", f"Falta o arquivo {path.name}. Repare o componente Audiovisual no Core.")
    return path


@contextmanager
def inference_lock(models: Path):
    """Serialize heavyweight inference; the OS releases this on cancellation."""
    # The host supplies a common path across versioned Core generations so an
    # update cannot start a second heavyweight inference alongside the old one.
    lock_path = Path(os.environ.get("JARVIS_AUDIO_LOCK_PATH", str(models.parent / ".inference.lock")))
    if not lock_path.is_absolute() or not lock_path.parent.is_dir():
        raise AudioError("runtime_unavailable", "O diretório de execução do áudio está indisponível. Repare o Core.")
    descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            import msvcrt

            class Overlapped(ctypes.Structure):
                _fields_ = [("Internal", ctypes.c_size_t), ("InternalHigh", ctypes.c_size_t),
                            ("Offset", wintypes.DWORD), ("OffsetHigh", wintypes.DWORD),
                            ("hEvent", wintypes.HANDLE)]

            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            lock = kernel.LockFileEx
            lock.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.DWORD,
                             wintypes.DWORD, wintypes.DWORD, ctypes.POINTER(Overlapped)]
            lock.restype = wintypes.BOOL
            handle = msvcrt.get_osfhandle(descriptor)
            overlapped = Overlapped()
            if not lock(handle, 3, 0, 1, 0, ctypes.byref(overlapped)):
                if ctypes.get_last_error() != 33:  # ERROR_LOCK_VIOLATION
                    raise ctypes.WinError(ctypes.get_last_error())
                progress("queued")
                overlapped = Overlapped()
                if not lock(handle, 2, 0, 1, 0, ctypes.byref(overlapped)):
                    raise ctypes.WinError(ctypes.get_last_error())
        else:
            import fcntl
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                progress("queued")
                fcntl.flock(descriptor, fcntl.LOCK_EX)
        yield
    finally:
        os.close(descriptor)


def write_wav(path: Path, samples: array, sample_rate: int) -> dict:
    if not samples or sample_rate < 1:
        raise AudioError("invalid_audio", "O modelo não gerou áudio. Tente novamente com outro texto ou descrição.")
    if any(not math.isfinite(value) for value in samples):
        raise AudioError("invalid_audio", "O modelo gerou valores de áudio inválidos. Tente novamente.")
    peak = max(abs(value) for value in samples)
    if peak < 1e-7:
        raise AudioError("invalid_audio", "O modelo gerou um áudio silencioso. Tente novamente.")
    scale = 0.98 / peak if peak > 0.98 else 1.0
    pcm = array("h", (round(max(-1.0, min(1.0, value * scale)) * 32767) for value in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    try:
        # Rust may provide an empty NamedTempFile. Only that empty regular file
        # may be reused; every populated artifact remains untouched.
        flags = os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
        try:
            descriptor = os.open(path, flags | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            if path.is_symlink():
                raise AudioError("output_exists", "A saída temporária é um link simbólico; escolha outro destino.")
            descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "r+b") as output:
            metadata = os.fstat(output.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_size != 0 or metadata.st_nlink != 1:
                raise AudioError("output_exists", "A saída temporária precisa ser um arquivo vazio e independente.")
            with wave.open(output, "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(sample_rate)
                wav.writeframes(pcm.tobytes())
    except FileExistsError as error:
        raise AudioError("output_exists", "A saída temporária já existe; escolha outro destino.") from error
    return {
        "duration": len(samples) / sample_rate,
        "sampleRate": sample_rate,
        "frames": len(samples),
        "output": str(path),
    }


def loop_background(samples: array, sample_rate: int, duration: float) -> array:
    """Extend a short seed using equal-power overlap, then fade both ends."""
    target = round(duration * sample_rate)
    if not samples or target < 1:
        raise AudioError("invalid_audio", "O modelo não gerou o trecho inicial da música. Tente novamente.")
    overlap = min(round(0.3 * sample_rate), len(samples) // 4)
    output = array("f", samples)
    while len(output) < target:
        if overlap:
            offset = len(output) - overlap
            for index in range(overlap):
                angle = (index + 1) / (overlap + 1) * math.pi / 2
                output[offset + index] = output[offset + index] * math.cos(angle) + samples[index] * math.sin(angle)
            output.extend(samples[overlap:])
        else:
            output.extend(samples)
    del output[target:]
    fade = min(round(0.25 * sample_rate), target // 2)
    for index in range(fade):
        gain = index / max(1, fade)
        output[index] *= gain
        output[-index - 1] *= gain
    return output


def narrate(request: dict) -> dict:
    # Lazy imports keep TTS (NumPy 2) separate from the MusicGen environment.
    import espeakng_loader
    import onnxruntime as ort
    from kokoro_onnx import Kokoro

    # A user's shell override must not replace the managed phonemizer library.
    os.environ["PHONEMIZER_ESPEAK_LIBRARY"] = str(espeakng_loader.get_library_path())
    model_root = Path(request["models"]) / "kokoro"
    model_path = require_file(model_root / "kokoro-v1.0.onnx")
    voices_path = require_file(model_root / "voices-v1.0.bin")
    progress("loading_voice")
    options = ort.SessionOptions()
    options.intra_op_num_threads = max(1, min(os.cpu_count() or 1, 4))
    options.inter_op_num_threads = 1
    session = ort.InferenceSession(str(model_path), options, providers=["CPUExecutionProvider"])
    kokoro = Kokoro.from_session(session, str(voices_path))
    voice = request.get("voice", "pm_alex")
    if voice not in kokoro.get_voices():
        raise AudioError("voice_missing", "A voz PT-BR escolhida não está instalada. Repare o componente Audiovisual no Core.")
    progress("narrating")
    samples, sample_rate = kokoro.create(
        request["text"].strip(), voice=voice, speed=request.get("speed", 1), lang="pt-br"
    )
    progress("writing")
    result = write_wav(Path(request["output"]), array("f", samples.tolist()), int(sample_rate))
    return {"engine": "kokoro-onnx", "device": "cpu", "voice": voice, "language": "pt-br", **result}


def music_device(torch, requested: str) -> str:
    if requested == "auto":
        # CUDA is supported by MusicGen. CPU remains the portable default;
        # MPS is opt-in because operator coverage varies by macOS/PyTorch.
        return "cuda" if torch.cuda.is_available() else "cpu"
    if requested == "cuda" and not torch.cuda.is_available():
        raise AudioError("device_unavailable", "CUDA não está disponível neste ambiente. Use cpu ou auto.")
    if requested == "mps" and not torch.backends.mps.is_available():
        raise AudioError("device_unavailable", "MPS não está disponível neste ambiente. Use cpu ou auto.")
    return requested


def model_warning_filter(record: logging.LogRecord) -> bool:
    """Keep actionable warnings; omit the pinned model's shared-config dumps."""
    message = record.getMessage()
    return not (message.startswith("Config of the ") and "is overwritten by shared" in message)


def music(request: dict) -> dict:
    import torch
    from transformers import AutoProcessor, MusicgenForConditionalGeneration

    model_root = Path(request["models"]) / "musicgen"
    require_file(model_root / "model.safetensors")
    require_file(model_root / "config.json")
    device = music_device(torch, request.get("device", "auto"))
    torch.set_num_threads(max(1, min(os.cpu_count() or 1, 4)))
    torch.manual_seed(request.get("seed", 0))
    progress("loading_music")
    processor = AutoProcessor.from_pretrained(str(model_root), local_files_only=True)
    config_logger = logging.getLogger("transformers.models.musicgen.modeling_musicgen")
    config_logger.addFilter(model_warning_filter)
    try:
        with warnings.catch_warnings():
            # Transformers 4.46's Encodec constructor wraps an existing integer
            # tensor with torch.tensor. This advisory is unrelated to inference.
            warnings.filterwarnings(
                "ignore", message="To copy construct from a tensor, it is recommended to use",
                category=UserWarning, module=r"transformers\.models\.encodec\.modeling_encodec",
            )
            # Classifier-free guidance uses an empty unconditional attention
            # mask, which SDPA rejects. Select its actual eager fallback upfront.
            model = MusicgenForConditionalGeneration.from_pretrained(
                str(model_root), local_files_only=True, use_safetensors=True,
                torch_dtype=torch.float32, attn_implementation="eager",
            ).to(device)
    finally:
        config_logger.removeFilter(model_warning_filter)
    model.eval()
    prompt = request["prompt"].strip() + ". Instrumental background music, no vocals, no speech."
    inputs = processor(text=[prompt], padding=True, return_tensors="pt")
    inputs = {name: value.to(device) for name, value in inputs.items()}
    seed_seconds = min(float(request["duration"]), 30.0)
    progress("generating_music")
    with torch.inference_mode():
        audio = model.generate(
            **inputs, max_new_tokens=max(1, math.ceil(seed_seconds * 50)),
            do_sample=True, guidance_scale=3.0,
        )
    # Explicit mono mix even if a future compatible model provides stereo.
    mono = audio[0].detach().to("cpu").float().mean(dim=0)
    sample_rate = int(model.config.audio_encoder.sampling_rate)
    progress("looping_music")
    samples = loop_background(array("f", mono.tolist()), sample_rate, float(request["duration"]))
    progress("writing")
    result = write_wav(Path(request["output"]), samples, sample_rate)
    return {
        "engine": "musicgen-small", "device": device, "seed": request.get("seed", 0),
        "license": "CC-BY-NC-4.0", "instrumental": True, **result,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True)
    args = parser.parse_args(argv)
    # Defense in depth: no inference path can fetch a model or use a global cache.
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
    os.environ["TOKENIZERS_PARALLELISM"] = "false"
    try:
        request = validate(json.loads(args.request))
        with inference_lock(Path(request["models"])):
            result = narrate(request) if request["action"] == "narrate" else music(request)
        # ASCII JSON escapes remain portable with Windows pipe encodings.
        print(json.dumps(result), flush=True)
        return 0
    except Exception as error:
        code = error.code if isinstance(error, AudioError) else "audio_generation_failed"
        print(json.dumps({"ok": False, "error": {"code": code, "message": str(error)}}), flush=True)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
