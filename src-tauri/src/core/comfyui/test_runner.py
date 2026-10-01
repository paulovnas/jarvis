"""Observable safety/output tests; optional integration uses real managed nodes."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import sys
from types import ModuleType

import pytest


@pytest.fixture(scope="module")
def runner() -> ModuleType:
    """Load the shipped script without invoking its command-line entry point."""
    path = Path(__file__).with_name("runner.py")
    spec = importlib.util.spec_from_file_location("jarvis_comfy_test", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def payload(tmp_path: Path) -> dict[str, object]:
    """Create canonical host-owned input and empty output paths."""
    source = tmp_path / "input.png"
    source.write_bytes(b"source")
    output = tmp_path / "output"
    output.mkdir()
    return {"inputs": [str(source.resolve())], "output": str(output.resolve())}


def test_optional_defaults_preserve_images(
    runner: ModuleType, payload: dict[str, object]
) -> None:
    validated = runner.validate(payload)
    assert validated.format == "png"
    assert validated.width is None and validated.height is None
    assert validated.upscale is False and validated.remove_background is False
    assert runner.output_size(validated, (640, 480)) == (640, 480)


@pytest.mark.parametrize("value", [None, [], "text", True, 4])
def test_request_must_be_an_object(runner: ModuleType, value: object) -> None:
    with pytest.raises(runner.ImageError, match="objeto JSON"):
        runner.validate(value)


@pytest.mark.parametrize(
    "key,value",
    [
        ("width", 0),
        ("width", -1),
        ("width", 4097),
        ("width", True),
        ("height", 1.5),
        ("height", "64"),
        ("format", "svg"),
        ("format", 1),
        ("upscale", 1),
        ("remove_background", "true"),
        ("workflow", {"class_type": "arbitrary code"}),
        ("python", "print('hi')"),
        ("inputs", []),
        ("inputs", ["relative.png"]),
        ("inputs", "image.png"),
        ("output", "relative"),
    ],
)
def test_invalid_requests_are_rejected_before_imports(
    runner: ModuleType,
    payload: dict[str, object],
    key: str,
    value: object,
) -> None:
    with pytest.raises(runner.ImageError):
        runner.validate({**payload, key: value})


def test_prevents_output_overwrites(
    runner: ModuleType, payload: dict[str, object]
) -> None:
    output = Path(str(payload["output"]))
    previous = output / "previous.png"
    previous.write_bytes(b"keep")
    with pytest.raises(runner.ImageError, match="preenchida"):
        runner.validate(payload)
    assert previous.read_bytes() == b"keep"


def test_rejects_missing_paths_and_directories_as_images(
    runner: ModuleType,
    payload: dict[str, object],
    tmp_path: Path,
) -> None:
    for source in (tmp_path, tmp_path / "missing"):
        with pytest.raises(runner.ImageError):
            runner.validate({**payload, "inputs": [str(source.resolve())]})


def test_input_count_duplicates_sizes_and_scopes_are_bounded(
    runner: ModuleType,
    payload: dict[str, object],
    tmp_path: Path,
) -> None:
    input_path = str(tmp_path / "input.png")
    for paths in ([input_path] * 2, [input_path] * 5):
        with pytest.raises(runner.ImageError):
            runner.validate({**payload, "inputs": paths})
    with (tmp_path / "input.png").open("wb") as file:
        file.truncate(runner.MAX_INPUT_BYTES + 1)
    with pytest.raises(runner.ImageError, match="32 MiB"):
        runner.validate(payload)
    inside = Path(str(payload["output"])) / "input.png"
    inside.write_bytes(b"keep")
    with pytest.raises(runner.ImageError):
        runner.validate({**payload, "inputs": [str(inside)]})


@pytest.mark.skipif(os.name == "nt", reason="Windows CI may not grant symlink creation")
def test_rejects_symlinks_in_inputs_and_output_parents(
    runner: ModuleType,
    payload: dict[str, object],
    tmp_path: Path,
) -> None:
    link = tmp_path / "linked.png"
    link.symlink_to(tmp_path / "input.png")
    parent = tmp_path / "linked-parent"
    parent.symlink_to(tmp_path, target_is_directory=True)
    for source in (link, parent / "input.png"):
        with pytest.raises(runner.ImageError, match="simbólicos"):
            runner.validate({**payload, "inputs": [str(source)]})
    with pytest.raises(runner.ImageError, match="simbólicos"):
        runner.validate({**payload, "output": str(parent / "output")})


def test_background_removal_requires_an_alpha_format(
    runner: ModuleType,
    payload: dict[str, object],
) -> None:
    with pytest.raises(runner.ImageError, match="transparente"):
        runner.validate({**payload, "remove_background": True, "format": "jpg"})


@pytest.mark.parametrize(
    "size,expected",
    [
        ({"width": 320}, (320, 240)),
        ({"height": 240}, (320, 240)),
        ({"width": 100, "height": 100}, (100, 100)),
        ({"width": 1280, "upscale": True}, (1280, 960)),
    ],
)
def test_dimensions_keep_aspect_ratio_or_explicit_requested_shape(
    runner: ModuleType,
    payload: dict[str, object],
    size: dict[str, object],
    expected: tuple[int, int],
) -> None:
    assert (
        runner.output_size(runner.validate({**payload, **size}), (640, 480)) == expected
    )


def test_interpolation_is_explicit_and_calculated_sizes_are_bounded(
    runner: ModuleType,
    payload: dict[str, object],
) -> None:
    with pytest.raises(runner.ImageError, match="interpolação"):
        runner.output_size(runner.validate({**payload, "width": 1280}), (640, 480))
    with pytest.raises(runner.ImageError, match="4096"):
        runner.output_size(
            runner.validate({**payload, "height": 4096, "upscale": True}), (640, 10)
        )


def test_private_environment_is_offline_and_turn_owned(
    runner: ModuleType,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("TRANSFORMERS_CACHE", "old-user-cache")
    runner.environment(tmp_path)
    assert os.environ["HF_HOME"] == str(tmp_path)
    assert os.environ["TMPDIR"] == str(tmp_path)
    assert os.environ["HF_HUB_OFFLINE"] == "1"
    assert os.environ["NUMBA_DISABLE_JIT"] == "1"
    assert os.environ["JOBLIB_MULTIPROCESSING"] == "0"
    assert "TRANSFORMERS_CACHE" not in os.environ


def test_local_events_record_bounded_actionable_failure(runner: ModuleType) -> None:
    events = runner.Events()
    events.send_sync("execution_error", {"exception_message": "x" * 3000})
    assert events.failure == "x" * 2000
    events.send_sync("execution_interrupted", {})
    assert events.failure == "Execução de imagem interrompida."


def test_cli_returns_structured_validation_error(
    runner: ModuleType,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    for argument in ("not-json", "[]", "x" * 32769):
        monkeypatch.setattr(sys, "argv", ["runner", "--request", argument])
        assert runner.main() == 1
        error = json.loads(capsys.readouterr().out)
        assert error["event"] == "error"
        assert error["code"] in ("invalid_request", "image_failed")


def test_cli_publishes_a_completed_report(
    runner: ModuleType,
    payload: dict[str, object],
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    report = {
        "engine": "comfyui",
        "images": [{"path": "image-01.png", "width": 64, "height": 64}],
    }
    monkeypatch.setattr(runner, "run", lambda request, package: report)
    monkeypatch.setattr(sys, "argv", ["runner", "--request", json.dumps(payload)])
    assert runner.main() == 0
    assert json.loads(capsys.readouterr().out) == {"event": "result", **report}


def test_health_loads_private_modules_without_processing_an_image(
    runner: ModuleType,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(runner, "bootstrap", lambda output, package: ())
    monkeypatch.setattr(
        runner.importlib, "import_module", lambda name: ModuleType(name)
    )
    monkeypatch.setattr(sys, "argv", ["runner", "--health"])
    assert runner.main() == 0
    assert json.loads(capsys.readouterr().out)["event"] == "health"


@pytest.mark.skipif(
    not os.environ.get("JARVIS_TEST_COMFYUI_PACKAGE"),
    reason="Requires the checked private ComfyUI runtime",
)
def test_official_executor_outputs_real_images_and_background_alpha(
    runner: ModuleType,
    tmp_path: Path,
) -> None:
    pillow = __import__("PIL.Image", fromlist=["Image"])
    draw = __import__("PIL.ImageDraw", fromlist=["ImageDraw"])
    source = tmp_path / "source.png"
    original = pillow.new("RGBA", (128, 128), (255, 255, 255, 0))
    draw.Draw(original).ellipse((20, 20, 108, 108), fill=(255, 0, 0, 255))
    original.save(source)
    original_bytes = source.read_bytes()
    package = Path(os.environ["JARVIS_TEST_COMFYUI_PACKAGE"])
    for format_value, remove in (("png", False), ("webp", True), ("jpg", False)):
        output = tmp_path / format_value
        output.mkdir()
        payload = runner.validate(
            {
                "inputs": [str(source)],
                "output": str(output),
                "width": 64,
                "format": format_value,
                "remove_background": remove,
            }
        )
        report = runner.run(payload, package)
        image = pillow.open(output / f"image-01.{format_value}")
        image.load()
        assert image.size == (64, 64)
        if format_value != "jpg":
            assert image.getchannel("A").getextrema() == (0, 255)
        else:
            assert image.mode == "RGB"
        stored = json.loads((output / "generation-report.json").read_text())
        assert stored == report
        assert stored["engine"] == "comfyui"
        graph = json.loads((output / "workflow.json").read_text())["graphs"][0]
        assert graph["load"]["class_type"] == "LoadImage"
        assert graph["save"]["class_type"] == "SaveImage"
        assert "background" in graph if remove else "background" not in graph
    assert source.read_bytes() == original_bytes
