"""Private, warm PT-BR speech worker; no downloads, microphone or agent loop."""
import json
import os
import sys
import wave
from pathlib import Path


def main():
    import espeakng_loader
    import onnxruntime as ort
    from kokoro_onnx import Kokoro

    os.environ["PHONEMIZER_ESPEAK_LIBRARY"] = str(espeakng_loader.get_library_path())
    models = Path(sys.argv[1]) / "kokoro"
    options = ort.SessionOptions()
    options.intra_op_num_threads = max(1, min(os.cpu_count() or 1, 4))
    options.inter_op_num_threads = 1
    session = ort.InferenceSession(str(models / "kokoro-v1.0.onnx"), options, providers=["CPUExecutionProvider"])
    voice = Kokoro.from_session(session, str(models / "voices-v1.0.bin"))
    print(json.dumps({"ready": True}), flush=True)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            if request["voice"] not in ("pf_dora", "pm_alex", "pm_santa") or not 0.75 <= request["speed"] <= 1.5:
                raise ValueError("Invalid voice settings")
            text = request["text"].strip()
            if not text or len(text) > 1500:
                raise ValueError("Invalid speech length")
            samples, rate = voice.create(text, voice=request["voice"], speed=request["speed"], lang="pt-br", trim=False, sentence_pause=0.25, clause_pause=0.12)
            import numpy as np
            # Retain consonants and a small natural pause before/after speech.
            samples = np.concatenate((np.zeros(round(rate * 0.08)), samples, np.zeros(round(rate * 0.12))))
            pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2")
            with wave.open(request["output"], "wb") as output:
                output.setnchannels(1)
                output.setsampwidth(2)
                output.setframerate(rate)
                output.writeframes(pcm.tobytes())
            print(json.dumps({"ok": True}), flush=True)
        except Exception as error:
            print(json.dumps({"ok": False, "error": str(error)}), flush=True)


if __name__ == "__main__":
    main()
