"""Opt-in real PT-BR TTS smoke; outputs a 16 kHz fixture for the Rust Whisper/VAD test."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import wave

parser = argparse.ArgumentParser()
parser.add_argument("runtime", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
entry = Path(__file__).resolve().parents[1] / "src-tauri/src/voice/speech.py"
wave_path = args.output / "speech.wav"
requests = [
    {"text": "Olá Jarvis. Quero conversar sobre o projeto e verificar os arquivos.", "voice": "pm_alex", "speed": 1, "output": str(wave_path)},
    {"text": "Teste inválido", "voice": "unknown", "speed": 1, "output": str(args.output / "invalid.wav")},
    {"text": "Podemos continuar nossa conversa.", "voice": "pf_dora", "speed": 1, "output": str(args.output / "second.wav")},
]
environment = {**os.environ, "PYTHONPATH": str(args.runtime / "packages_tts"), "OMP_NUM_THREADS": "4", "OPENBLAS_NUM_THREADS": "4"}
result = subprocess.run([str(args.runtime / "python/bin/python3.11"), str(entry), str(args.runtime / "models")], input="".join(json.dumps(request) + "\n" for request in requests), capture_output=True, text=True, env=environment, timeout=90, check=True)
responses = [json.loads(line) for line in result.stdout.splitlines()]
assert responses[0] == {"ready": True}, responses
assert responses[1] == {"ok": True} and responses[2]["ok"] is False and responses[3] == {"ok": True}, responses
assert not (args.output / "invalid.wav").exists()
# Resample only the synthetic fixture, never user audio.
import sys
sys.path.insert(0, str(args.runtime / "packages_tts"))
import numpy as np
with wave.open(str(wave_path), "rb") as source:
    assert source.getnchannels() == 1 and source.getsampwidth() == 2
    rate = source.getframerate()
    samples = np.frombuffer(source.readframes(source.getnframes()), dtype="<i2").astype(np.float32)
    assert np.max(np.abs(samples)) > 100
    assert np.count_nonzero(samples[:round(rate * .08)]) == 0
target = np.arange(round(len(samples) * 16000 / rate)) * rate / 16000
pcm = np.interp(target, np.arange(len(samples)), samples).astype("<i2")
with wave.open(str(args.output / "speech-16k.wav"), "wb") as output:
    output.setnchannels(1); output.setsampwidth(2); output.setframerate(16000); output.writeframes(pcm.tobytes())
print(f"Real Kokoro PT-BR worker passed: two voices, warm reuse, invalid-input recovery, WAV padding; {len(pcm) / 16000:.2f}s fixture.")
