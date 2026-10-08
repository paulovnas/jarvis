"""JSON adapter for the complete, privately installed OpenMontage package.

The coding agent directs production; this adapter executes registered tools and
the upstream pipeline/checkpoint contracts, never source supplied in a prompt.
"""
from __future__ import annotations

import argparse
import contextlib
import functools
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import threading

COMPOSER_WORKDIRS: list[Path] = []

class BridgeError(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def inside(root: Path, value: str | Path, base: Path | None = None) -> Path:
    if not isinstance(value, (str, Path)) or not str(value) or "\0" in str(value):
        raise BridgeError("invalid_path", "Informe um caminho válido dentro do projeto.")
    candidate = Path(value)
    resolved = (candidate if candidate.is_absolute() else (base or root) / candidate).resolve()
    if not resolved.is_relative_to(root.resolve()):
        raise BridgeError("project_scope", "O caminho sai do projeto. Os arquivos foram preservados.")
    return resolved


def sanitize(value, secrets: list[str]):
    if isinstance(value, str):
        for secret in secrets:
            if len(secret) >= 6:
                value = value.replace(secret, "[redacted]")
        return re.sub(r"(?i)(bearer\s+)[\w.\-]+", r"\1[redacted]", value)
    if isinstance(value, dict):
        return {key: "[redacted]" if re.search(r"(?i)(api[_-]?key|secret|token|password|authorization)", key)
                else sanitize(item, secrets) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [sanitize(item, secrets) for item in value]
    return value


class RedactingStream:
    """Stream progress, retaining a bounded suffix so split secrets stay hidden."""
    def __init__(self, stream, secrets):
        self.stream, self.secrets, self.pending = stream, secrets, ""
        self.lock = threading.Lock()

    def write(self, text):
        with self.lock:
            self.pending += str(text)
            # Emit complete lines only; subprocess jobs retain their own bounded log.
            while "\n" in self.pending:
                line, self.pending = self.pending.split("\n", 1)
                self.stream.write(sanitize(line, self.secrets) + "\n")
            if len(self.pending) > 65536:
                self.pending = "[progress line omitted]" + self.pending[-65536:]
            self.stream.flush()
        return len(str(text))

    def flush(self):
        # Upstream libraries flush partial writes. Keep the incomplete line so
        # a secret split across write/flush calls is redacted as one value.
        self.stream.flush()

    def finish(self):
        with self.lock:
            if self.pending:
                self.stream.write(sanitize(self.pending, self.secrets))
                self.pending = ""
            self.stream.flush()

    def isatty(self):
        return False


PATH_KEYS = {"path", "paths", "file", "files", "filename", "src", "source", "destination", "workspace", "directory", "input", "output",
             "entry", "entrypoint", "assets_root", "face_tracking_json", "primary_audio", "secondary_audio", "image", "video", "audio", "images", "videos", "clips", "title_font"}
FORBIDDEN_KEYS = {"approved", "human_approved", "native_approved", "api_key", "token", "secret",
                  "authorization", "shell_command", "python_code", "executable", "env"}


def scoped_arguments(value, root: Path, directory: Path, key="", write_outputs=True, creative=False):
    """Constrain nested asset manifests as well as top-level input/output paths."""
    normalized = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", key).lower()
    creative = creative or normalized in {"edit_decisions", "scene_plan", "terminal_scene", "composition"}
    if normalized in FORBIDDEN_KEYS or (normalized == "command" and not creative):
        raise BridgeError("reserved_argument", f"{key} é reservado à integração nativa.")
    if isinstance(value, dict):
        return {name: scoped_arguments(item, root, directory, name, write_outputs, creative) for name, item in value.items()}
    if isinstance(value, list):
        return [scoped_arguments(item, root, directory, key, write_outputs, creative) for item in value]
    path_key = normalized in PATH_KEYS or normalized.endswith(("_path", "_paths", "_dir", "_dirs", "_file", "_files", "_filename", "_directory", "_src", "_image", "_images", "_video", "_videos", "_audio", "_audios", "_url", "_urls"))
    if isinstance(value, str) and path_key and value:
        if re.match(r"https?://", value) or value.startswith("data:"):
            if normalized in {"output", "output_path", "destination", "workspace_path"}:
                raise BridgeError("invalid_path", "O destino precisa ser um arquivo local do projeto.")
            return value
        # Artifact IDs (e.g. asset_manifest cuts source='shot-1') are not filenames.
        if normalized in {"source", "input", "title_font"} and not any(c in value for c in "/\\."):
            return value
        if value.lower().startswith("file:"):
            from urllib.parse import unquote, urlsplit
            parsed = urlsplit(value)
            if parsed.netloc not in {"", "localhost"}:
                raise BridgeError("project_scope", "Um asset local não pode apontar para outro host.")
            value = unquote(parsed.path)
            if os.name == "nt" and re.match(r"^/[A-Za-z]:", value):
                value = value[1:]
        candidate = inside(root, value, directory)
        destination = normalized in {"output", "destination", "workspace", "workspace_path", "project_dir"} or normalized.startswith(("output_", "destination_"))
        if write_outputs and destination:
            candidate = inside(directory, candidate)
            empty_directory = normalized.endswith("_dir") and candidate.is_dir() and not any(candidate.iterdir())
            if normalized not in {"workspace", "workspace_path", "project_dir"} and candidate.exists() and not empty_directory:
                raise BridgeError("output_exists", "O destino já existe. Use um novo nome; o resultado anterior foi preservado.")
        return str(candidate)
    return value


def tool_info(tool, *, check_dependencies=True):
    if not check_dependencies:
        # Catalog discovery must not import GPU/vision SDKs or inventory system fonts.
        info = {name: getattr(tool, name, None) for name in
                ("name", "capability", "provider", "dependencies", "capabilities", "side_effects")}
        info.update({"runtime": getattr(tool.runtime, "value", str(tool.runtime)),
                     "stability": getattr(tool.stability, "value", str(tool.stability)),
                     "status": "not_checked"})
        return info
    try:
        info = tool.get_info()
    except Exception as exc:
        info = {name: getattr(tool, name, None) for name in
                ("name", "capability", "provider", "dependencies", "capabilities", "side_effects",
                 "input_schema", "output_schema", "supports", "agent_skills")}
        info.update({"runtime": getattr(tool.runtime, "value", str(tool.runtime)),
                     "stability": getattr(tool.stability, "value", str(tool.stability)),
                     "resource_profile": {"network_required": tool.resource_profile.network_required},
                     "status": "unavailable", "unavailableReason": str(exc)})
    if getattr(tool, "name", None) == "piper_tts":
        model = Path(os.environ.get("PIPER_MODEL_PATH") or ".missing-piper-model")
        if not model.is_file() or not Path(str(model) + ".json").is_file():
            info.update({"status": "unavailable", "unavailableReason": "Configure PIPER_MODEL_PATH com um modelo .onnx e seu .onnx.json na configuração do OpenMontage."})
    if info.get("status") == "unavailable" and not info.get("unavailableReason"):
        try:
            tool.check_dependencies()
        except Exception as exc:
            info["unavailableReason"] = str(exc)
    return info


def needs_approval(tool, arguments: dict) -> tuple[bool, dict]:
    info = tool_info(tool)
    try:
        preflight = tool.dry_run(arguments)
    except (KeyError, ValueError, TypeError) as exc:
        preflight = {"estimated_cost_usd": None, "cost_status": "arguments_required",
                     "quote_reason": str(exc), "status": info["status"]}
    # Native prerequisites (e.g. Piper's configured ONNX voice) are stricter
    # than upstream's executable-only status check.
    preflight["status"] = info.get("status", preflight.get("status", "unavailable"))
    runtime = info.get("runtime")
    def remote_input(value):
        if isinstance(value, dict):
            return any(remote_input(item) for item in value.values())
        if isinstance(value, list):
            return any(remote_input(item) for item in value)
        return isinstance(value, str) and bool(re.match(r"https?://", value))
    network = bool(info.get("resource_profile", {}).get("network_required")) or remote_input(arguments)
    name = getattr(tool, "name", "")
    if name == "hyperframes_compose" and arguments.get("operation") in {
        "render", "render_existing", "lint", "validate", "inspect", "check", "scaffold_workspace"
    }:
        # Upstream's generic network profile assumes npx/CDN downloads. The
        # native installation supplies the pinned CLI and exact GSAP locally.
        # Registry installs and unaudited operations retain their network gate.
        dispatcher = Path(os.environ.get("JARVIS_OPENMONTAGE_NPX") or ".missing-dispatcher")
        local_gsap = dispatcher.parent.parent / "node_modules" / "gsap" / "dist" / "gsap.min.js"
        if (os.environ.get("HYPERFRAMES_MANAGED_VERSION")
                and os.environ.get("npm_config_offline") == "true"
                and dispatcher.is_file() and local_gsap.is_file()):
            network = remote_input(arguments)
    decisions = arguments.get("edit_decisions") or arguments.get("composition_data") or {}
    atelier = decisions.get("composition_mode") == "atelier" or decisions.get("renderer_family") == "bespoke"
    stock_remotion = (name == "remotion_caption_burn"
                     or (name == "video_compose" and (arguments.get("operation") == "remotion_render"
                         or (arguments.get("operation") == "render" and decisions.get("render_runtime") == "remotion" and not atelier))))
    # Stock compositions import Google Fonts at module initialization. An
    # authored atelier entry using local fonts has no such implicit request.
    network = network or stock_remotion
    authored = []
    if name == "hyperframes_compose" and arguments.get("workspace_path"):
        authored.append(Path(arguments["workspace_path"]) / "index.html")
    if name == "video_compose" and atelier and isinstance(decisions.get("bespoke"), dict) and decisions["bespoke"].get("entry"):
        authored.append(Path(decisions["bespoke"]["entry"]))
    for entry in authored:
        if entry.is_absolute() and entry.is_file():
            with entry.open("r", encoding="utf-8", errors="replace") as file:
                network = network or bool(re.search(r"https?://", file.read(256 * 1024)))
    network = network or (runtime == "local_gpu" and os.environ.get("JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS") == "1")
    paid = runtime in {"api", "hybrid"} or any(dep.startswith("env:") for dep in info.get("dependencies", []))
    estimate = preflight.get("estimated_cost_usd")
    unknown = runtime not in {"local", "local_gpu", "api", "hybrid"}
    approved = paid or network or unknown or estimate is None or (isinstance(estimate, (int, float)) and estimate > 0)
    return approved, {**preflight, "requiresApproval": approved, "network": network or paid,
                      "sideEffects": info.get("side_effects", []), "runtime": runtime,
                      "provider": info.get("provider"), "stability": info.get("stability")}


def require_paid_tools(preflight):
    estimate = preflight.get("estimated_cost_usd")
    positive = isinstance(estimate, (int, float)) and not isinstance(estimate, bool) and estimate > 0
    unpriced = preflight.get("runtime") == "hybrid" and preflight.get("cost_status") in {"quote_required", "user_quote", "user_quote_pending_approval"}
    # Hybrid legacy generators may call an API directly, without dispatching
    # another BaseTool. Native approval cannot override the user's opt-out.
    if (preflight.get("runtime") == "api" or positive or unpriced) and os.environ.get("JARVIS_OPENMONTAGE_ALLOW_PAID") != "1":
        raise BridgeError("paid_tools_disabled", "As ferramentas com custo ou de API estão desabilitadas na configuração do OpenMontage.")


def configure(package: Path):
    sys.path.insert(0, str(package))
    # Credentials come exclusively from the native managed configuration.
    import tools.tool_registry as tool_registry
    tool_registry.ToolRegistry._load_dotenv = staticmethod(lambda: None)
    import lib.env_loader as env_loader
    env_loader.load_env = lambda project_root=None: None
    from lib.config_model import OpenMontageConfig
    original_load = OpenMontageConfig.load.__func__
    native_config = Path(os.environ["OPENMONTAGE_CONFIG"]) if os.environ.get("OPENMONTAGE_CONFIG") else package / "config.yaml"
    OpenMontageConfig.load = classmethod(lambda cls, config_path=None: original_load(cls, config_path or native_config))
    # The managed renderer is pinned and installed before inference, including npm's offline cache.
    from tools.video.hyperframes_compose import HyperFramesCompose
    if os.environ.get("HYPERFRAMES_MANAGED_VERSION"):
        HyperFramesCompose._npm_resolve_cache = {"version": os.environ["HYPERFRAMES_MANAGED_VERSION"]}
        local_gsap = package.parent / "node_modules" / "gsap" / "dist" / "gsap.min.js"
        if local_gsap.is_file():
            original_html = HyperFramesCompose._generate_index_html
            def html(self, *args, **kwargs):
                return original_html(self, *args, **kwargs).replace("https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js", "assets/vendor/gsap.min.js")
            HyperFramesCompose._generate_index_html = html
            original_scaffold = HyperFramesCompose._scaffold
            def scaffold(self, inputs):
                import shutil
                workspace = Path(inputs["workspace_path"])
                if (workspace / "index.html").exists():
                    raise BridgeError("output_exists", "A composição já existe. Use render_existing para preservar o conteúdo ou escolha uma nova pasta.")
                result = original_scaffold(self, inputs)
                if result.success:
                    target = workspace / "assets/vendor/gsap.min.js"
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(local_gsap, target)
                    result.artifacts.append(str(target))
                return result
            HyperFramesCompose._scaffold = scaffold
    registry = tool_registry.ToolRegistry()
    registry.discover()
    return registry, OpenMontageConfig.load()


def managed_render_command(command):
    """Use native managed interpreters rather than host or relocated launchers."""
    if not isinstance(command, (list, tuple)) or not command:
        return command
    executable = Path(str(command[0])).name.lower()
    if executable.removesuffix(".exe") == "manim":
        manager = os.environ.get("JARVIS_OPENMONTAGE_MANIM_MANAGER")
        prefix = os.environ.get("JARVIS_OPENMONTAGE_MANIM_PREFIX")
        python = os.environ.get("JARVIS_OPENMONTAGE_MANIM_PYTHON")
        if manager or prefix or python:
            if not manager or not prefix or not python or not Path(manager).is_file() or not Path(prefix).is_dir() or not Path(python).is_file() or not Path(python).resolve().is_relative_to(Path(prefix).resolve()):
                raise BridgeError("renderer_unavailable", "O runtime Manim está incompleto. Reinstale Animações Manim na configuração do OpenMontage.")
            # Micromamba sets the native DLL/library paths, including on Windows.
            if str(command[0]) == executable or Path(str(command[0])).resolve().is_relative_to(Path(prefix).resolve()):
                cache = Path(os.environ.get("XDG_CACHE_HOME", str(Path.cwd() / ".cache")))
                return [manager, "--no-rc", "--no-env", "run", "--root-prefix", str(cache / "mamba/root"), "--prefix", prefix, python, "-I", "-B", "-m", "manim", *command[1:]]
    modules = {"piper": "piper", "manim": "manim", "yt-dlp": "yt_dlp"}
    module = modules.get(executable.removesuffix(".exe"))
    if module and os.environ.get("VIRTUAL_ENV"):
        import shutil
        resolved = shutil.which(str(command[0]))
        managed = Path(os.environ["VIRTUAL_ENV"]).resolve()
        if resolved and Path(resolved).resolve().parent in {managed / "Scripts", managed / "bin"}:
            # distlib .exe launchers embed the staging venv interpreter path.
            # Use the relocated native interpreter, without modifying PE files.
            return [sys.executable, "-m", module, *command[1:]]
    if executable not in {"npx", "npx.cmd", "npm", "npm.cmd"}:
        return command
    node = os.environ.get("JARVIS_OPENMONTAGE_NODE")
    dispatcher = os.environ.get("JARVIS_OPENMONTAGE_NPM" if executable.startswith("npm") else "JARVIS_OPENMONTAGE_NPX")
    if not node and not dispatcher:
        return command
    if not node or not dispatcher or not Path(node).is_file() or not Path(dispatcher).is_file():
        raise BridgeError("renderer_unavailable", "O runtime de renderização está incompleto. Repare o OpenMontage no Core.")
    return [node, dispatcher, *command[1:]]


def guard_renderer_processes():
    # Cover BaseTool.run_command and direct subprocess.run/Popen calls upstream.
    original = subprocess.Popen
    class ManagedPopen(original):
        def __init__(self, args, *positional, **kwargs):
            # Registered Python API tools keep their credentials; renderers and
            # other child CLIs cannot leak those credentials into authored media.
            environment = kwargs.get("env") if kwargs.get("env") is not None else os.environ
            allowed = re.compile(r"^(?:PATH|HOME|USERPROFILE|LOCALAPPDATA|APPDATA|TMPDIR|TMP|TEMP|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_.*|PYTHON.*|VIRTUAL_ENV|NODE.*|NPM.*|HYPERFRAMES.*|PRODUCER_.*|REMOTION.*|GSAP.*|FFMPEG.*|XDG_.*|OPENMONTAGE_PROJECTS_DIR|JARVIS_OPENMONTAGE_(?:NODE|NPX|NPM|MANIM_(?:MANAGER|PREFIX|PYTHON))|HF_HOME|HUGGINGFACE_HUB_CACHE|HF_HUB_OFFLINE|TRANSFORMERS_OFFLINE|TORCH_HOME|CUDA.*|ROCM.*|OMP_NUM_THREADS|PIPER_MODEL_PATH|BLENDER_PATH|MUSIC_LIBRARY_DIR)$", re.IGNORECASE)
            kwargs["env"] = {key: value for key, value in environment.items() if allowed.match(key) and not re.search(r"KEY|TOKEN|SECRET|PASSWORD|AUTHORIZATION|CREDENTIAL", key, re.IGNORECASE)}
            super().__init__(managed_render_command(args), *positional, **kwargs)
    subprocess.Popen = ManagedPopen


@contextlib.contextmanager
def without_dotenv_files():
    """Upstream BaseTool also loads .env during import; native vault is authoritative."""
    original = Path.is_file
    Path.is_file = lambda path: False if path.name == ".env" else original(path)
    try:
        yield
    finally:
        Path.is_file = original


def guard_model_downloads():
    """Enforce the native opt-in even for a nested tool using requests directly."""
    if os.environ.get("JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS") == "1":
        return
    import requests.sessions
    import urllib.request
    original = requests.sessions.Session.request

    def check(url):
        from urllib.parse import urlsplit
        parsed = urlsplit(getattr(url, "full_url", str(url)))
        model_host = (parsed.hostname or "").lower() in {"huggingface.co", "hf.co", "modelscope.cn", "www.modelscope.cn"}
        weights = re.search(r"\.(safetensors|gguf|ckpt|onnx|pth|pt)(?:$|[/?])", parsed.path, re.IGNORECASE)
        if model_host or weights:
            raise BridgeError("model_downloads_disabled", "Downloads de modelos estão desabilitados. Habilite-os na configuração do OpenMontage ou forneça os pesos locais.")

    @functools.wraps(original)
    def request(session, method, url, *args, **kwargs):
        check(url)
        return original(session, method, url, *args, **kwargs)

    requests.sessions.Session.request = request
    original_open, original_retrieve = urllib.request.urlopen, urllib.request.urlretrieve
    def urlopen(url, *args, **kwargs):
        check(url)
        return original_open(url, *args, **kwargs)
    def urlretrieve(url, *args, **kwargs):
        check(url)
        return original_retrieve(url, *args, **kwargs)
    urllib.request.urlopen, urllib.request.urlretrieve = urlopen, urlretrieve
    # Torch may have imported an alias before discovery finished.
    if "torch.hub" in sys.modules:
        sys.modules["torch.hub"].urlopen = urlopen


def stage_composer(package: Path, directory: Path):
    """Project-local editable templates, with immutable managed dependencies."""
    import shutil
    import tempfile
    source = package / "remotion-composer"
    if not (source / "node_modules").is_dir():
        raise BridgeError("renderer_unavailable", "O compositor Remotion não está instalado. Repare o OpenMontage no Core.")
    work = Path(tempfile.mkdtemp(prefix=".jarvis-openmontage-render-", dir=directory))
    COMPOSER_WORKDIRS.append(work)
    composer = work / "remotion-composer"
    shutil.copytree(source, composer, ignore=shutil.ignore_patterns("node_modules", "projects", ".cache"))
    dependencies = composer / "node_modules"
    try:
        dependencies.symlink_to(source / "node_modules", target_is_directory=True)
    except OSError:
        if os.name != "nt":
            raise
        # NTFS junctions do not require Windows developer mode or elevation.
        subprocess.run(["cmd", "/d", "/c", "mklink", "/J", str(dependencies), str(source / "node_modules")], check=True, capture_output=True)
    import tools.video.video_compose as video_compose
    import tools.video.remotion_caption_burn as caption_burn
    for module in [video_compose, caption_burn]:
        module.__file__ = str(work / "tools" / "video" / Path(module.__file__).name)
    return work


def guard_nested_tools(registry, approved: bool, root: Path, directory: Path, package: Path | None = None):
    """Selectors cannot silently escalate from a local tool to a paid provider."""
    from tools.base_tool import BaseTool

    composer_staged = False
    def wrap(cls):
        original = cls.__dict__.get("execute")
        if original is None or getattr(original, "_jarvis_guarded", False):
            return

        @functools.wraps(original)
        def guarded(self, inputs):
            nonlocal composer_staged
            inputs = scoped_arguments(inputs, root, directory)
            if self.name == "piper_tts":
                inputs = resolve_piper_model(inputs, root, directory)
                if not Path(inputs["model"]).is_file() or not Path(inputs["model"] + ".json").is_file():
                    raise BridgeError("tool_unavailable", "Configure um modelo Piper .onnx e seu .onnx.json antes de gerar narração local.")
            required, preflight = needs_approval(self, inputs)
            if required and not approved:
                raise BridgeError("approval_required", f"A ferramenta {self.name} requer autorização explícita antes de usar rede ou um provedor.")
            require_paid_tools(preflight)
            if package and not composer_staged and self.name in {"video_compose", "remotion_caption_burn"}:
                # Encoding/muxing needs no browser or composer working copy.
                decisions = inputs.get("edit_decisions") or {}
                if self.name != "video_compose" or inputs.get("operation") == "remotion_render" or (inputs.get("operation") == "render" and decisions.get("render_runtime") == "remotion"):
                    stage_composer(package, directory)
                    composer_staged = True
            return original(self, inputs)

        guarded._jarvis_guarded = True
        cls.execute = guarded

    pending = list(BaseTool.__subclasses__())
    while pending:
        cls = pending.pop()
        wrap(cls)
        pending.extend(cls.__subclasses__())


def resolve_piper_model(arguments, root: Path, directory: Path):
    arguments = dict(arguments)
    selected = arguments.get("model")
    native = os.environ.get("PIPER_MODEL_PATH", "")
    if selected and ("/" in selected or "\\" in selected or selected.endswith(".onnx")):
        # A previously resolved native voice can be outside the user's project.
        # Only that exact, explicitly configured path has this privilege.
        arguments["model"] = native if native and selected == native else str(inside(root, selected, directory))
    else:
        arguments["model"] = native
    return arguments


def quoted_preflight(tool, preflight: dict, quote, approved: bool):
    if quote is None:
        return preflight
    if isinstance(quote, bool) or not isinstance(quote, (int, float)) or not math.isfinite(quote) or quote < 0:
        raise BridgeError("invalid_quote", "costQuoteUsd precisa ser uma estimativa finita e não negativa em dólares.")
    if preflight.get("estimated_cost_usd") is not None:
        return preflight
    if preflight.get("cost_status") != "quote_required":
        raise BridgeError("invalid_quote", "Uma cotação não corrige argumentos ausentes ou inválidos. Consulte o schema da ferramenta.")
    preflight = {**preflight, "estimated_cost_usd": quote, "requiresApproval": True,
                 "cost_status": "user_quote" if approved else "user_quote_pending_approval",
                 "costIsEstimated": True, "quoteSource": "user", "quoteIsProviderVerified": False}
    if approved:
        from tools.provider_pricing import PriceQuoteRequired
        original = tool.estimate_cost
        def estimate(inputs):
            try:
                return original(inputs)
            except PriceQuoteRequired:
                return quote
        tool.estimate_cost = estimate
    return preflight


def production(directory: Path):
    from lib import checkpoint
    marker_path = directory / "project.json"
    if not marker_path.is_file():
        raise BridgeError("project_not_initialized", "Inicialize esta produção com video_run action=init antes de continuar.")
    marker = json.loads(marker_path.read_text(encoding="utf-8"))
    if marker.get("project_id") != directory.name:
        raise BridgeError("project_identity", "A identidade da produção não corresponde à pasta selecionada.")
    return checkpoint, marker


def project_status(directory: Path):
    checkpoint, marker = production(directory)
    pipeline = marker["pipeline_type"]
    stages = checkpoint.get_pipeline_stages(pipeline)
    return {"project": marker, "stages": [checkpoint.read_checkpoint(directory.parent, directory.name, stage)
                or {"stage": stage, "status": "pending"} for stage in stages],
            "nextStage": checkpoint.get_next_stage(directory.parent, directory.name, pipeline),
            "costs": json.loads((directory / "cost_log.json").read_text(encoding="utf-8"))
                if (directory / "cost_log.json").is_file() else None}


def review_video(root: Path, path: Path, arguments: dict):
    from tools.video.video_compose import VideoCompose
    path = inside(root, path)
    if path.suffix.lower() != ".mp4" or not path.is_file():
        raise BridgeError("invalid_video", "Selecione um MP4 produzido dentro do projeto.")
    probe = subprocess.run(["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(path)],
                           capture_output=True, text=True, timeout=30, check=False)
    try:
        metadata = json.loads(probe.stdout)
        stream = next(item for item in metadata["streams"] if item.get("codec_type") == "video")
        duration = float(metadata["format"]["duration"])
        valid = probe.returncode == 0 and math.isfinite(duration) and duration > 0 and int(stream["width"]) > 0 and int(stream["height"]) > 0
    except (ValueError, KeyError, StopIteration, TypeError):
        valid = False
    if not valid:
        raise BridgeError("invalid_video", "O render não contém um stream de vídeo válido com duração positiva. O arquivo foi preservado.")
    review = VideoCompose()._run_final_review(path, edit_decisions=arguments.get("edit_decisions"),
                proposal_packet=arguments.get("proposal_packet"),
                narration_transcript_path=arguments.get("narration_transcript_path"), script_text=arguments.get("script_text"))
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return {"path": path.relative_to(root).as_posix(), "sha256": digest.hexdigest(), "probe": metadata, "review": review,
            "verified": True, "visualReviewRequired": True}


def dispatch(request, registry, config):
    root = Path(request["root"]).resolve()
    directory = inside(root, request.get("directory", root))
    action = request["action"]
    args = request.get("arguments") or {}
    if not isinstance(args, dict):
        raise BridgeError("invalid_arguments", "arguments precisa ser um objeto JSON.")
    approved = request.get("nativeApproved") is True
    if action == "tools":
        name = request.get("tool")
        if name:
            tool = registry.get(name)
            if tool is None:
                raise BridgeError("unknown_tool", "A ferramenta não existe no catálogo instalado do OpenMontage.")
            info = tool_info(tool)
            return {"tool": info, "preflight": needs_approval(tool, {})[1]}
        tools = [registry.get(name) for name in sorted(registry.list_all())]
        capability = request.get("capability")
        if capability:
            tools = [tool for tool in tools if capability == tool.capability or capability in getattr(tool, "capabilities", [])]
        offset = max(0, int(request.get("offset", 0)))
        page = [tool_info(tool, check_dependencies=False) for tool in tools[offset:offset + 30]]
        return {"tools": page, "total": len(tools), "nextOffset": offset + len(page) if offset + len(page) < len(tools) else None,
                "capabilities": sorted({registry.get(name).capability for name in registry.list_all()})}
    if action in {"preflight", "tool"}:
        tool = registry.get(request.get("tool", ""))
        if tool is None:
            raise BridgeError("unknown_tool", "Consulte video_tools e use o nome exato de uma ferramenta instalada.")
        args = scoped_arguments(args, root, directory)
        if tool.name == "piper_tts":
            args = resolve_piper_model(args, root, directory)
        read_operations = {"info", "probe", "inspect", "analyze", "list", "search", "status", "doctor", "check", "validate", "lint", "scaffold_workspace", "add_block"}
        if ("output_path" in tool.input_schema.get("properties", {}) and not args.get("output_path")
                and args.get("operation") not in read_operations):
            raise BridgeError("output_required", "Informe um novo output_path dentro da produção para preservar resultados anteriores.")
        import jsonschema
        jsonschema.validate(args, tool.input_schema)
        required, preflight = needs_approval(tool, args)
        preflight = quoted_preflight(tool, preflight, request.get("costQuoteUsd"), approved)
        required = bool(preflight["requiresApproval"])
        if tool.name == "piper_tts" and args.get("model"):
            import shutil
            model = Path(args["model"])
            if shutil.which("piper") and model.is_file() and Path(str(model) + ".json").is_file():
                preflight["status"] = "available"
        if action == "preflight":
            return preflight
        production(directory)
        if preflight["status"] == "unavailable":
            raise BridgeError("tool_unavailable", f"{tool.name} está indisponível. Consulte as dependências em video_tools e configure o provedor ou o recurso necessário no Core.")
        if required and not approved:
            raise BridgeError("approval_required", "A operação requer autorização explícita do usuário.")
        require_paid_tools(preflight)
        args.setdefault("project_dir", str(directory))
        args.setdefault("project_id", directory.name)
        guard_nested_tools(registry, approved, root, directory, Path(request["package"]))
        from tools.cost_tracker import CostTracker
        tracker = CostTracker(budget_total_usd=config.budget.total_usd, reserve_pct=config.budget.reserve_pct,
                   require_approval_for_new_paid_tool=config.budget.require_approval_for_new_paid_tool, mode=config.budget.mode,
                   single_action_approval_usd=float("inf") if approved else config.budget.single_action_approval_usd,
                   cost_log_path=directory / "cost_log.json")
        estimated = preflight.get("estimated_cost_usd")
        if estimated is None and required:
            raise BridgeError("quote_required", "O preço deste modelo depende do plano da conta. Consulte a cotação e envie costQuoteUsd no nível de video_run para autorização explícita; ela será registrada como estimativa do usuário, não como cobrança confirmada.")
        estimate = float(estimated or 0)
        if not math.isfinite(estimate) or estimate < 0:
            raise BridgeError("invalid_quote", "A estimativa de custo não é válida.")
        entry = tracker.estimate(tool.name, str(args.get("operation", action)), estimate)
        if approved:
            tracker.approve_tool(tool.name)
        tracker.reserve(entry)
        try:
            result = tool.execute(args)
        except Exception:
            # An interrupted/failed API request may already have incurred a charge.
            tracker.reconcile(entry, estimate, success=False)
            raise
        cost = result.cost_usd
        actual = float(cost) if isinstance(cost, (int, float)) and math.isfinite(cost) and cost >= 0 else estimate
        tracker.reconcile(entry, actual, success=result.success)
        artifacts = []
        videos = []
        for artifact in result.artifacts:
            artifact_path = inside(root, artifact, directory)
            artifacts.append(artifact_path.relative_to(root).as_posix())
            if result.success and artifact_path.suffix.lower() == ".mp4" and artifact_path.is_file():
                videos.append(review_video(root, artifact_path, args))
        return {"success": result.success, "data": result.data, "artifacts": artifacts,
                "error": result.error, "costUsd": cost, "costIsEstimated": True,
                "durationSeconds": result.duration_seconds, "preflight": preflight,
                "costSnapshot": tracker.cost_snapshot(), "videos": videos}
    if action == "init":
        from lib.pipeline_loader import load_pipeline_readonly
        from lib.checkpoint import init_project
        pipeline = request.get("pipeline")
        if not isinstance(pipeline, str) or not re.fullmatch(r"[a-z0-9-]+", pipeline):
            raise BridgeError("invalid_pipeline", "Escolha um pipeline apresentado em video_docs topic=pipelines.")
        load_pipeline_readonly(pipeline)
        if directory == root or not re.fullmatch(r"[A-Za-z0-9_.-]+", directory.name):
            raise BridgeError("invalid_project", "Crie a produção em uma subpasta do projeto, com um nome simples.")
        if directory.exists() and any(directory.iterdir()):
            raise BridgeError("project_exists", "A pasta já contém arquivos. Use status para retomar ou uma pasta nova; nada foi sobrescrito.")
        init_project(directory.name, title=str(args.get("title") or directory.name), pipeline_type=pipeline,
                     pipeline_dir=directory.parent, style_playbook=args.get("style_playbook"))
        return project_status(directory)
    if action == "status":
        return project_status(directory)
    if action in {"checkpoint", "approve"}:
        if action == "checkpoint":
            args = scoped_arguments(args, root, directory, write_outputs=False)
        checkpoint, marker = production(directory)
        stage = args.get("stage")
        if not isinstance(stage, str) or stage not in checkpoint.get_pipeline_stages(marker["pipeline_type"]):
            raise BridgeError("invalid_checkpoint", "Escolha uma etapa do pipeline desta produção.")
        if action == "approve":
            if not approved:
                raise BridgeError("approval_required", "Somente uma autorização nativa do usuário pode aprovar esta etapa.")
            existing = checkpoint.read_checkpoint(directory.parent, directory.name, stage)
            if not existing or existing.get("status") != "awaiting_human":
                raise BridgeError("invalid_checkpoint", "A etapa precisa estar aguardando aprovação antes de ser aprovada.")
            args = {**existing, "status": "completed"}
        if "human_approved" in args and action != "approve":
            raise BridgeError("reserved_argument", "A IA não pode aprovar uma etapa pelo conteúdo dos argumentos.")
        saved = checkpoint.write_checkpoint(directory.parent, directory.name, stage, args.get("status", "in_progress"),
                    args.get("artifacts", {}), pipeline_type=marker["pipeline_type"],
                    style_playbook=marker.get("style_playbook"), checkpoint_policy=config.checkpoint.policy.value,
                    human_approval_required=bool(args.get("human_approval_required")), human_approved=action == "approve",
                    review=args.get("review"), cost_snapshot=args.get("cost_snapshot"), error=args.get("error"),
                    metadata=args.get("metadata"))
        return {"checkpoint": saved.relative_to(root).as_posix(), **project_status(directory)}
    if action == "review":
        args = scoped_arguments(args, root, directory, write_outputs=False)
        return review_video(root, inside(root, args.get("output_path", ""), directory), args)
    raise BridgeError("unknown_action", "Ação OpenMontage desconhecida.")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--request", required=True)
    parsed = parser.parse_args()
    request = json.loads(Path(parsed.request).read_text(encoding="utf-8"))
    package = Path(request["package"]).resolve()
    root = Path(request["root"]).resolve()
    directory = inside(root, request.get("directory", root))
    secrets = [value for key, value in os.environ.items() if re.search(r"(?i)(api.?key|secret|token|password)", key)]
    stream = RedactingStream(sys.stdout, secrets)
    result = None
    try:
        os.environ["OPENMONTAGE_PROJECTS_DIR"] = str(directory.parent)
        os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
        with contextlib.redirect_stdout(stream), contextlib.redirect_stderr(stream):
            with without_dotenv_files():
                registry, config = configure(package)
            guard_model_downloads()
            guard_renderer_processes()
            if request["action"] not in {"tools", "preflight"}:
                directory.mkdir(parents=True, exist_ok=True)
            os.chdir(directory if directory.is_dir() else root)
            result = {"success": True, "action": request["action"], **dispatch(request, registry, config)}
    except Exception as exc:
        result = {"success": False, "error": {"code": getattr(exc, "code", "openmontage_error"),
                  "message": str(exc)}, "action": request.get("action")}
    finally:
        stream.finish()
        import shutil
        for work in COMPOSER_WORKDIRS:
            shutil.rmtree(work, ignore_errors=True)
    result = sanitize(result, secrets)
    result_path = request.get("resultPath")
    if result_path:
        target = inside(root, result_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        # Native request supplies an unpredictable new receipt path.
        with target.open("x", encoding="utf-8") as file:
            json.dump(result, file, ensure_ascii=False, default=str)
    print(json.dumps(result, ensure_ascii=False, default=str), flush=True)
    return 0 if result.get("success") is not False else 1


if __name__ == "__main__":
    raise SystemExit(main())
