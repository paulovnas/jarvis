"""Fixed offline image graphs executed by the official ComfyUI PromptExecutor.

The Rust host owns read-only input attachments and an empty writable staging
directory. This runner never accepts user graphs, arbitrary code or node names.
"""

from __future__ import annotations

import argparse
from contextlib import redirect_stdout
from dataclasses import dataclass
import hashlib
import importlib
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
from types import ModuleType
from typing import Protocol, cast
import warnings


COMFY_VERSION = "0.3.8"
MODEL_SHA256 = "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8"
MODEL_SIZE = 4_574_861
MAX_DIMENSION = 4096
MAX_INPUT_BYTES = 32 * 1024 * 1024


class ImageError(Exception):
    """An actionable structured image-tool error."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


@dataclass(frozen=True)
class Request:
    """The complete validated, host-owned image operation."""

    inputs: tuple[Path, ...]
    output: Path
    width: int | None
    height: int | None
    format: str
    upscale: bool
    remove_background: bool


class Executor(Protocol):
    """Small typed boundary to the dynamically loaded upstream executor."""

    success: bool

    def execute(
        self,
        prompt: dict[str, object],
        prompt_id: str,
        extra_data: dict[str, object],
        execute_outputs: list[str],
    ) -> None: ...


def progress(phase: str, **details: object) -> None:
    """Emit one bounded JSON event without upstream console noise."""
    print(json.dumps({"event": "progress", "phase": phase, **details}), flush=True)


def canonical_path(value: object, directory: bool = False) -> Path:
    """Reject relative paths and symlinks in every path component."""
    if not isinstance(value, str) or not Path(value).is_absolute():
        raise ImageError("invalid_request", "Informe um caminho absoluto de imagem.")
    path = Path(value)
    try:
        if any(part.is_symlink() for part in (path, *path.parents)):
            raise ImageError("invalid_request", "Links simbólicos não são aceitos.")
        mode = path.stat().st_mode
        if not (stat.S_ISDIR(mode) if directory else stat.S_ISREG(mode)):
            raise ImageError("invalid_request", "O caminho de imagem é inválido.")
        return path.resolve(strict=True)
    except OSError as exc:
        raise ImageError(
            "invalid_request", "O arquivo de imagem está indisponível."
        ) from exc


def dimension(value: object) -> int | None:
    """Read an optional bounded output dimension."""
    if value is None:
        return None
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not 1 <= value <= MAX_DIMENSION
    ):
        raise ImageError(
            "invalid_request", "As dimensões devem ser inteiros de 1 a 4096."
        )
    return value


def validate(value: object) -> Request:
    """Validate all request and filesystem boundaries before importing Torch."""
    if not isinstance(value, dict):
        raise ImageError(
            "invalid_request", "A solicitação de imagem precisa ser um objeto JSON."
        )
    request = cast(dict[str, object], value)
    allowed = {
        "inputs",
        "output",
        "width",
        "height",
        "format",
        "upscale",
        "remove_background",
    }
    if set(request) - allowed:
        raise ImageError(
            "invalid_request", "A solicitação contém uma opção desconhecida."
        )
    sources = request.get("inputs")
    if not isinstance(sources, list) or not 1 <= len(sources) <= 4:
        raise ImageError("invalid_request", "Selecione de 1 a 4 imagens.")
    inputs = tuple(canonical_path(source) for source in sources)
    if len(set(inputs)) != len(inputs) or any(
        path.stat().st_size > MAX_INPUT_BYTES for path in inputs
    ):
        raise ImageError("invalid_request", "Use imagens distintas de até 32 MiB cada.")
    output = canonical_path(request.get("output"), directory=True)
    if any(output.iterdir()):
        raise ImageError(
            "output_exists",
            "A pasta temporária já está preenchida; use uma pasta vazia.",
        )
    if any(path.is_relative_to(output) for path in inputs):
        raise ImageError(
            "invalid_request", "As entradas precisam ficar fora da pasta de saída."
        )
    format_value = request.get("format", "png")
    if not isinstance(format_value, str) or format_value not in ("png", "jpg", "webp"):
        raise ImageError("invalid_request", "Escolha PNG, JPG ou WEBP para a saída.")
    for key in ("upscale", "remove_background"):
        if not isinstance(request.get(key, False), bool):
            raise ImageError(
                "invalid_request", f"{key} precisa ser verdadeiro ou falso."
            )
    if request.get("remove_background", False) and format_value == "jpg":
        raise ImageError(
            "invalid_request", "Use PNG ou WEBP para preservar o fundo transparente."
        )
    return Request(
        inputs,
        output,
        dimension(request.get("width")),
        dimension(request.get("height")),
        format_value,
        bool(request.get("upscale", False)),
        bool(request.get("remove_background", False)),
    )


def environment(output: Path) -> None:
    """Keep third-party caches inside the writable per-turn staging directory."""
    for key in (
        "HF_HOME",
        "NUMBA_CACHE_DIR",
        "XDG_CACHE_HOME",
        "TMPDIR",
        "TEMP",
        "TMP",
    ):
        os.environ[key] = str(output)
    os.environ.update(
        {
            "HF_HUB_OFFLINE": "1",
            "TRANSFORMERS_OFFLINE": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONNOUSERSITE": "1",
            "JOBLIB_MULTIPROCESSING": "0",
            "NUMBA_DISABLE_JIT": "1",
            "TOKENIZERS_PARALLELISM": "false",
            "OMP_NUM_THREADS": "2",
            "MKL_NUM_THREADS": "2",
        }
    )
    os.environ.pop("TRANSFORMERS_CACHE", None)
    sys.dont_write_bytecode = True


class Events:
    """Local event sink; no listening server or browser is created."""

    client_id: str | None = "jarvis"
    last_node_id: str | None = None
    failure: str | None = None

    def send_sync(
        self, event: str, data: dict[str, object], client_id: str | None = None
    ) -> None:
        """Keep only actionable bounded execution metadata."""
        if event in ("execution_error", "execution_interrupted"):
            self.failure = str(
                data.get("exception_message", "Execução de imagem interrompida.")
            )[:2000]
        elif event == "executing":
            # Upstream output is redirected to stderr, including these events.
            print(
                json.dumps(
                    {"event": "progress", "phase": "workflow", "node": data.get("node")}
                ),
                file=sys.__stdout__,
                flush=True,
            )


def bootstrap(output: Path, package: Path) -> tuple[ModuleType, ModuleType, ModuleType]:
    """Import the pinned official executor with CPU-only, fixed-node settings."""
    environment(output)
    source = package / "source"
    sys.path.insert(0, str(source))
    sys.argv = [
        "jarvis-comfy",
        "--cpu",
        "--disable-xformers",
        "--disable-all-custom-nodes",
        "--disable-auto-launch",
        "--disable-metadata",
    ]
    importlib.import_module("comfy.options").enable_args_parsing()
    folders = importlib.import_module("folder_paths")
    for key in ("input", "output", "temp", "user"):
        getattr(folders, f"set_{key}_directory")(str(output))
    # Complete Dynamo before ComfyUI starts einops/torchvision imports; the
    # alternative order creates a circular import in these pinned versions.
    importlib.import_module("torch._dynamo")
    nodes = importlib.import_module("nodes")
    execution = importlib.import_module("execution")
    torch = importlib.import_module("torch")
    torch.set_num_threads(2)
    return nodes, execution, torch


def register_adapters(nodes: object, torch: object, package: Path) -> None:
    """Register only Jarvis-owned fixed alpha and offline U2-Net graph nodes."""
    node_module = cast(ModuleType, nodes)
    torch_module = cast(ModuleType, torch)

    class PreserveAlpha:
        RETURN_TYPES = ("IMAGE",)
        FUNCTION = "join"
        CATEGORY = "jarvis/image"

        @classmethod
        def INPUT_TYPES(cls) -> dict[str, object]:
            return {"required": {"image": ("IMAGE",), "mask": ("MASK",)}}

        def join(self, image: object, mask: object) -> tuple[object]:
            tensor = torch_module.as_tensor(image)
            alpha = 1.0 - torch_module.as_tensor(mask).unsqueeze(-1)
            if alpha.shape[1:3] != tensor.shape[1:3]:
                alpha = torch_module.ones_like(tensor[:, :, :, :1])
            return (torch_module.cat((tensor, alpha), dim=-1),)

    class RemoveBackground:
        RETURN_TYPES = ("IMAGE",)
        FUNCTION = "remove"
        CATEGORY = "jarvis/image"

        @classmethod
        def INPUT_TYPES(cls) -> dict[str, object]:
            return {"required": {"image": ("IMAGE",)}}

        def remove(self, image: object) -> tuple[object]:
            model = package / "models/u2netp.onnx"
            if (
                model.stat().st_size != MODEL_SIZE
                or hashlib.sha256(model.read_bytes()).hexdigest() != MODEL_SHA256
            ):
                raise ImageError(
                    "models_missing",
                    "O modelo de remoção de fundo está incompleto. Repare ComfyUI no Core.",
                )
            ort = importlib.import_module("onnxruntime")
            np = importlib.import_module("numpy")
            pillow = importlib.import_module("PIL.Image")
            options = ort.SessionOptions()
            options.intra_op_num_threads = 2
            options.inter_op_num_threads = 1
            session = ort.InferenceSession(
                str(model), sess_options=options, providers=["CPUExecutionProvider"]
            )
            raw = (
                (torch_module.as_tensor(image)[0].cpu().numpy() * 255)
                .clip(0, 255)
                .astype(np.uint8)
            )
            original = pillow.fromarray(raw)
            # The U2-Net contract used by rembg: RGB 320x320, ImageNet channel
            # normalization, then the first predicted foreground map. Running
            # the checked ONNX model directly avoids importing unused OpenCV,
            # alpha-matting and JIT libraries into every private image process.
            sample = np.asarray(
                original.convert("RGB").resize((320, 320), pillow.Resampling.LANCZOS),
                dtype=np.float32,
            )
            sample = sample / max(float(sample.max()), 1e-6)
            sample = (
                sample - np.array([0.485, 0.456, 0.406], dtype=np.float32)
            ) / np.array([0.229, 0.224, 0.225], dtype=np.float32)
            tensor = np.ascontiguousarray(
                sample.transpose(2, 0, 1)[None], dtype=np.float32
            )
            predicted = session.run(None, {session.get_inputs()[0].name: tensor})[0][
                0, 0
            ]
            low, high = float(predicted.min()), float(predicted.max())
            if not np.isfinite(predicted).all() or high - low <= 1e-8:
                raise ImageError(
                    "background_failed",
                    "O modelo não conseguiu separar o fundo desta imagem.",
                )
            mask = pillow.fromarray(
                ((predicted - low) * 255 / (high - low)).clip(0, 255).astype(np.uint8)
            )
            mask = mask.resize(original.size, pillow.Resampling.LANCZOS)
            alpha = np.asarray(mask, dtype=np.float32)
            if "A" in original.getbands():
                alpha = (
                    alpha * np.asarray(original.getchannel("A"), dtype=np.float32) / 255
                )
            cutout = original.convert("RGBA")
            cutout.putalpha(pillow.fromarray(alpha.astype(np.uint8)))
            return (
                torch_module.from_numpy(
                    np.array(cutout).astype(np.float32) / 255
                ).unsqueeze(0),
            )

    # Discard the broad upstream catalog. User/custom node initialization never runs.
    mappings = {
        "LoadImage": node_module.LoadImage,
        "ImageScale": node_module.ImageScale,
        "SaveImage": node_module.SaveImage,
        "JarvisPreserveAlpha": PreserveAlpha,
        "JarvisRemoveBackground": RemoveBackground,
    }
    setattr(node_module, "NODE_CLASS_MAPPINGS", mappings)


def inspect_image(path: Path) -> tuple[int, int]:
    """Reject oversized, animated and unsupported input images before decoding."""
    pillow = importlib.import_module("PIL.Image")
    setattr(pillow, "MAX_IMAGE_PIXELS", MAX_DIMENSION * MAX_DIMENSION)
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", pillow.DecompressionBombWarning)
            with pillow.open(path) as image:
                width, height = image.size
                if (
                    image.format not in ("PNG", "JPEG", "WEBP")
                    or getattr(image, "n_frames", 1) != 1
                ):
                    raise ImageError(
                        "invalid_image", "Use uma imagem estática PNG, JPG ou WEBP."
                    )
                if not 1 <= width <= MAX_DIMENSION or not 1 <= height <= MAX_DIMENSION:
                    raise ImageError(
                        "invalid_image",
                        "A imagem original deve ter até 4096 pixels por dimensão.",
                    )
                image.verify()
                return int(width), int(height)
    except (pillow.DecompressionBombWarning, pillow.DecompressionBombError) as exc:
        raise ImageError(
            "invalid_image", "A imagem original excede o limite de pixels permitido."
        ) from exc


def output_size(request: Request, original: tuple[int, int]) -> tuple[int, int]:
    """Preserve aspect ratio when only one dimension was requested."""
    width, height = original
    target_width = request.width or (
        max(1, round(width * request.height / height)) if request.height else width
    )
    target_height = request.height or (
        max(1, round(height * request.width / width)) if request.width else height
    )
    if max(target_width, target_height) > MAX_DIMENSION:
        raise ImageError(
            "invalid_request", "As dimensões calculadas excedem 4096 pixels."
        )
    if not request.upscale and (target_width > width or target_height > height):
        raise ImageError(
            "invalid_request", "Ative a interpolação para ampliar a imagem."
        )
    return target_width, target_height


def run(request: Request, package: Path) -> dict[str, object]:
    """Execute images sequentially to bound peak tensor memory and publish a report."""
    sizes = [output_size(request, inspect_image(path)) for path in request.inputs]
    progress("loading")
    with redirect_stdout(sys.stderr):
        nodes, execution, torch = bootstrap(request.output, package)
        register_adapters(nodes, torch, package)
    workflows: list[dict[str, object]] = []
    results: list[dict[str, object]] = []
    for index, (source, size) in enumerate(
        zip(request.inputs, sizes, strict=True), start=1
    ):
        width, height = size
        graph: dict[str, object] = {
            "load": {"class_type": "LoadImage", "inputs": {"image": str(source)}},
            "alpha": {
                "class_type": "JarvisPreserveAlpha",
                "inputs": {"image": ["load", 0], "mask": ["load", 1]},
            },
        }
        current = "alpha"
        if request.remove_background:
            graph["background"] = {
                "class_type": "JarvisRemoveBackground",
                "inputs": {"image": [current, 0]},
            }
            current = "background"
        graph["scale"] = {
            "class_type": "ImageScale",
            "inputs": {
                "image": [current, 0],
                "upscale_method": "lanczos",
                "width": width,
                "height": height,
                "crop": "disabled",
            },
        }
        prefix = f"image-{index:02d}"
        graph["save"] = {
            "class_type": "SaveImage",
            "inputs": {"images": ["scale", 0], "filename_prefix": f"{prefix}-comfy"},
        }
        events = Events()
        with redirect_stdout(sys.stderr):
            executor = cast(Executor, execution.PromptExecutor(events))
            executor.execute(graph, prefix, {"client_id": "jarvis"}, ["save"])
        if not executor.success or events.failure:
            raise ImageError(
                "workflow_failed", events.failure or "ComfyUI não concluiu a imagem."
            )
        intermediate = list(request.output.glob(f"{prefix}-comfy_*.png"))
        if len(intermediate) != 1:
            raise ImageError(
                "workflow_failed", "ComfyUI não confirmou uma saída única."
            )
        destination = request.output / f"{prefix}.{request.format}"
        if destination.exists() or destination.is_symlink():
            raise ImageError("output_exists", "A imagem de saída já existe.")
        if request.format == "png":
            intermediate[0].rename(destination)
        else:
            pillow = importlib.import_module("PIL.Image")
            with pillow.open(intermediate[0]) as image:
                if request.format == "jpg":
                    background = pillow.new("RGB", image.size, "white")
                    background.paste(image, mask=image.getchannel("A"))
                    background.save(destination, "JPEG", quality=95)
                else:
                    image.save(destination, "WEBP", lossless=True)
            intermediate[0].unlink()
        results.append({"path": destination.name, "width": width, "height": height})
        workflows.append(graph)
        progress("image_ready", index=index, total=len(request.inputs))
        del executor
    report: dict[str, object] = {
        "engine": "comfyui",
        "version": COMFY_VERSION,
        "operations": {
            "resize": request.width is not None or request.height is not None,
            "interpolation": request.upscale,
            "remove_background": request.remove_background,
            "format": request.format,
        },
        "images": results,
    }
    (request.output / "workflow.json").write_text(
        json.dumps({"engine": "comfyui", "graphs": workflows}, indent=2),
        encoding="utf-8",
    )
    (request.output / "generation-report.json").write_text(
        json.dumps(report, indent=2), encoding="utf-8"
    )
    return report


def main() -> int:
    """CLI boundary consumed by the cancellable native process lifecycle."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--request")
    parser.add_argument("--health", action="store_true")
    args = parser.parse_args()
    package = Path(__file__).resolve().parent
    try:
        if args.health:
            with tempfile.TemporaryDirectory(
                prefix="jarvis-comfy-health-"
            ) as temporary:
                with redirect_stdout(sys.stderr):
                    bootstrap(Path(temporary), package)
                    importlib.import_module("onnxruntime")
                print(
                    json.dumps(
                        {
                            "event": "health",
                            "engine": "comfyui",
                            "version": COMFY_VERSION,
                        }
                    )
                )
            return 0
        if not args.request or len(args.request) > 32_768:
            raise ImageError(
                "invalid_request", "Informe uma solicitação de imagem de até 32 KiB."
            )
        request = validate(json.loads(args.request))
        report = run(request, package)
        print(json.dumps({"event": "result", **report}), flush=True)
        return 0
    except Exception as exc:  # Terminal boundary: return recoverable tool errors.
        print(
            json.dumps(
                {
                    "event": "error",
                    "code": getattr(exc, "code", "image_failed"),
                    "message": str(exc)[:2000],
                }
            ),
            flush=True,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
