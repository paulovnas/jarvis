"""Run: python -m unittest discover -s src-tauri/src/core -p openmontage_bridge_test.py.

With OPENMONTAGE_TEST_PACKAGE set, also exercises the real upstream registry,
checkpoint contracts and an FFmpeg MP4, without touching user installations.
"""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace

BRIDGE = Path(__file__).with_name("openmontage_bridge.py")
spec = importlib.util.spec_from_file_location("jarvis_openmontage_bridge", BRIDGE)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class BridgeBoundaries(unittest.TestCase):
    def test_flushing_split_secret_does_not_publish_partial_credentials(self):
        output = io.StringIO()
        stream = bridge.RedactingStream(output, ["sk-long-provider-secret"])
        stream.write("request sk-long-")
        stream.flush()
        self.assertEqual(output.getvalue(), "")
        stream.write("provider-secret failed\n")
        self.assertEqual(output.getvalue(), "request [redacted] failed\n")
        stream.write("last sk-long-provider-secret")
        stream.finish()
        self.assertEqual(output.getvalue(), "request [redacted] failed\nlast [redacted]")
    def test_scopes_nested_media_and_preserves_existing_outputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            production = root / "production"
            production.mkdir()
            outside = root.parent / "outside.mp4"
            for value in [str(outside), "../../outside.mp4"]:
                with self.assertRaises(bridge.BridgeError):
                    bridge.scoped_arguments({"asset_manifest": {"assets": [{"path": value}]}}, root, production)
                for key in ["src", "backgroundSrc", "audioSrc", "inputPaths", "entry", "images", "face_tracking_json", "assets_root"]:
                    with self.assertRaises(bridge.BridgeError):
                        bridge.scoped_arguments({"edit_decisions": {"scenes": [{key: [value] if key == "inputPaths" else value}]}}, root, production)
                with self.assertRaises(bridge.BridgeError):
                    bridge.scoped_arguments({"edit_decisions": {"scenes": [{"src": outside.as_uri()}]}}, root, production)
                for key in ["clips", "reference_videos", "reference_audios", "image_urls"]:
                    with self.assertRaises(bridge.BridgeError):
                        bridge.scoped_arguments({key: [value]}, root, production)
            previous = production / "final.mp4"
            previous.write_bytes(b"previous render")
            with self.assertRaises(bridge.BridgeError):
                bridge.scoped_arguments({"output_path": "final.mp4"}, root, production)
            self.assertEqual(previous.read_bytes(), b"previous render")
            self.assertEqual(bridge.scoped_arguments({"output_path": "next.mp4"}, root, production)["output_path"], str(production / "next.mp4"))
            for key in ["output_path", "output_dir", "workspace_path", "project_dir"]:
                with self.assertRaises(bridge.BridgeError):
                    bridge.scoped_arguments({key: "../other-assignment/result"}, root, production)
            self.assertEqual(bridge.scoped_arguments({"input_path": "../product.png"}, root, production)["input_path"], str(root / "product.png"))
            (production / "frames").mkdir()
            self.assertEqual(bridge.scoped_arguments({"output_dir":"frames"}, root, production)["output_dir"], str(production / "frames"))
            (production / "frames/previous.jpg").write_bytes(b"previous")
            with self.assertRaises(bridge.BridgeError):
                bridge.scoped_arguments({"output_dir":"frames"}, root, production)
            self.assertEqual(bridge.scoped_arguments({"output_path": "final.mp4"}, root, production, write_outputs=False)["output_path"], str(previous))
            if hasattr(os, "symlink"):
                (production / "escape").symlink_to(root.parent, target_is_directory=True)
                with self.assertRaises(bridge.BridgeError):
                    bridge.scoped_arguments({"output_path": "escape/outside.mp4"}, root, production)

    def test_model_cannot_supply_approval_secrets_or_arbitrary_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for key in ["approved", "human_approved", "api_key", "env", "python_code", "command"]:
                with self.assertRaises(bridge.BridgeError):
                    bridge.scoped_arguments({"nested": {key: "value"}}, root, root)
            scene = {"edit_decisions": {"cuts": [{"scene_type": "terminal_scene", "command": "git clone product"}]}}
            self.assertEqual(bridge.scoped_arguments(scene, root, root), scene)
            self.assertEqual(bridge.sanitize({"error": "request sk-secret-value failed", "api_key": "sk-secret-value"}, ["sk-secret-value"]),
                             {"error": "request [redacted] failed", "api_key": "[redacted]"})

    def test_trusted_metadata_requires_explicit_network_paid_or_unknown_approval(self):
        def tool(runtime="local", network=False, estimate=0, dependencies=None):
            return SimpleNamespace(get_info=lambda: {"runtime": runtime, "resource_profile": {"network_required": network},
                  "dependencies": dependencies or [], "side_effects": [], "provider": "test", "status": "available"},
                  dry_run=lambda args: {"estimated_cost_usd": estimate, "status": "available"})
        self.assertFalse(bridge.needs_approval(tool(), {})[0])
        self.assertTrue(bridge.needs_approval(tool(), {"edit_decisions": {"src": "https://example.com/media.mp4"}})[0])
        for candidate in [tool(runtime="api"), tool(runtime="hybrid"), tool(network=True), tool(estimate=1),
                          tool(estimate=None), tool(runtime="unknown"), tool(dependencies=["env:PROVIDER_KEY"])]:
            self.assertTrue(bridge.needs_approval(candidate, {})[0])
        renderer = tool()
        renderer.name = "video_compose"
        self.assertTrue(bridge.needs_approval(renderer, {"operation": "remotion_render"})[0])
        self.assertFalse(bridge.needs_approval(renderer, {"operation": "render", "edit_decisions": {"render_runtime": "remotion", "composition_mode": "atelier"}})[0])
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "index.tsx"
            entry.write_text("const font='https://example.com/font.woff2';")
            self.assertTrue(bridge.needs_approval(renderer, {"operation": "render", "edit_decisions": {"render_runtime": "remotion", "composition_mode": "atelier", "bespoke": {"entry": str(entry)}}})[0])

    def test_native_piper_voice_and_project_voice_are_distinct_authorities(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve() / "project"
            root.mkdir()
            native = root.parent / "native-voice.onnx"
            native.write_bytes(b"voice")
            previous = os.environ.get("PIPER_MODEL_PATH")
            os.environ["PIPER_MODEL_PATH"] = str(native)
            try:
                selected = bridge.resolve_piper_model({}, root, root)
                self.assertEqual(selected["model"], str(native))
                self.assertEqual(bridge.resolve_piper_model(selected, root, root), selected)
                with self.assertRaises(bridge.BridgeError):
                    bridge.resolve_piper_model({"model": str(root.parent / "another.onnx")}, root, root)
                info_tool = SimpleNamespace(name="piper_tts", get_info=lambda: {"status": "available", "runtime":"local"}, dry_run=lambda args:{"status":"available", "estimated_cost_usd":0})
                self.assertEqual(bridge.tool_info(info_tool)["status"], "unavailable")
                self.assertEqual(bridge.needs_approval(info_tool, {})[1]["status"], "unavailable")
                Path(str(native) + ".json").write_text("{}")
                self.assertEqual(bridge.tool_info(info_tool)["status"], "available")
                self.assertEqual(bridge.resolve_piper_model({"model": "project-voice.onnx"}, root, root)["model"], str(root / "project-voice.onnx"))
            finally:
                if previous is None:
                    os.environ.pop("PIPER_MODEL_PATH", None)
                else:
                    os.environ["PIPER_MODEL_PATH"] = previous

    def test_managed_hyperframes_only_skips_implicit_dependency_network(self):
        tool = SimpleNamespace(name="hyperframes_compose", get_info=lambda: {
            "runtime":"local", "resource_profile":{"network_required":True},
            "dependencies":[], "side_effects":[], "status":"available"},
            dry_run=lambda args:{"estimated_cost_usd":0, "status":"available"})
        with tempfile.TemporaryDirectory() as temporary:
            generation = Path(temporary).resolve()
            dispatcher = generation / "bin/jarvis-npx.cjs"
            dispatcher.parent.mkdir()
            dispatcher.write_text("// managed dispatcher")
            gsap = generation / "node_modules/gsap/dist/gsap.min.js"
            gsap.parent.mkdir(parents=True)
            gsap.write_text("// managed GSAP")
            keys = ["JARVIS_OPENMONTAGE_NPX", "HYPERFRAMES_MANAGED_VERSION", "npm_config_offline"]
            previous = {key:os.environ.get(key) for key in keys}
            try:
                for key in keys:
                    os.environ.pop(key, None)
                self.assertTrue(bridge.needs_approval(tool, {"operation":"render_existing"})[0])
                os.environ.update({"JARVIS_OPENMONTAGE_NPX":str(dispatcher), "HYPERFRAMES_MANAGED_VERSION":"0.8.140", "npm_config_offline":"true"})
                workspace = generation / "workspace"
                workspace.mkdir()
                index = workspace / "index.html"
                index.write_text("<script src='assets/vendor/gsap.min.js'></script>")
                local = {"operation":"render_existing", "workspace_path":str(workspace)}
                self.assertFalse(bridge.needs_approval(tool, local)[0])
                self.assertTrue(bridge.needs_approval(tool, {**local, "operation":"add_block"})[0])
                self.assertTrue(bridge.needs_approval(tool, {**local, "asset_manifest":{"src":"https://example.com/image.png"}})[0])
                index.write_text("<script src='https://example.com/runtime.js'></script>")
                self.assertTrue(bridge.needs_approval(tool, local)[0])
                index.write_text("<h1>Local</h1>")
                gsap.unlink()
                self.assertTrue(bridge.needs_approval(tool, local)[0])
            finally:
                for key, value in previous.items():
                    if value is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = value

    def test_cost_quote_is_finite_and_does_not_grant_approval(self):
        preflight = {"estimated_cost_usd": None, "cost_status": "quote_required"}
        tool = SimpleNamespace()
        for quote in [True, -1, float("nan"), float("inf"), "1.00"]:
            with self.assertRaises(bridge.BridgeError):
                bridge.quoted_preflight(tool, preflight, quote, False)
        pending = bridge.quoted_preflight(tool, preflight, 0.5, False)
        self.assertEqual(pending["estimated_cost_usd"], 0.5)
        self.assertEqual(pending["cost_status"], "user_quote_pending_approval")
        self.assertTrue(pending["requiresApproval"])
        self.assertFalse(pending["quoteIsProviderVerified"])
        self.assertFalse(hasattr(tool, "estimate_cost"))
        with self.assertRaises(bridge.BridgeError):
            bridge.quoted_preflight(tool, {"estimated_cost_usd": None, "cost_status": "arguments_required"}, 1, True)

    def test_direct_renderer_process_uses_managed_node_and_preserves_cwd(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            dispatcher = root / "dispatcher.py"
            dispatcher.write_text("import json, os, sys; print(json.dumps({'args':sys.argv[1:], 'cwd':os.getcwd(), 'environment':dict(os.environ)}))")
            keys = {key: os.environ.get(key) for key in ["JARVIS_OPENMONTAGE_NODE", "JARVIS_OPENMONTAGE_NPX", "JARVIS_OPENMONTAGE_NPM"]}
            original = subprocess.Popen
            os.environ.update({"JARVIS_OPENMONTAGE_NODE": sys.executable, "JARVIS_OPENMONTAGE_NPX": str(dispatcher), "JARVIS_OPENMONTAGE_NPM": str(dispatcher)})
            try:
                bridge.guard_renderer_processes()
                result = subprocess.run(["npx.cmd", "remotion", "render", "entry.ts"], cwd=root, env={**os.environ, "OPENAI_API_KEY":"sk-child-secret", "GOOGLE_APPLICATION_CREDENTIALS":"private.json", "NODE_FAKE_TOKEN":"unsafe-override", "FFMPEG_PATH":"managed/ffmpeg"}, text=True, capture_output=True, check=True)
                observed = json.loads(result.stdout)
                self.assertEqual(observed["args"], ["remotion", "render", "entry.ts"])
                self.assertEqual(observed["cwd"], str(root))
                self.assertNotIn("OPENAI_API_KEY", observed["environment"])
                self.assertNotIn("GOOGLE_APPLICATION_CREDENTIALS", observed["environment"])
                self.assertNotIn("NODE_FAKE_TOKEN", observed["environment"])
                self.assertEqual(observed["environment"]["FFMPEG_PATH"], "managed/ffmpeg")
                result = subprocess.run([str(root / "npm.cmd"), "--version"], cwd=root, text=True, capture_output=True, check=True)
                self.assertEqual(json.loads(result.stdout)["args"], ["--version"])
                dispatcher.unlink()
                with self.assertRaises(bridge.BridgeError):
                    subprocess.run(["npx", "hyperframes", "render"])
            finally:
                subprocess.Popen = original
                for key, value in keys.items():
                    if value is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = value

    def test_managed_console_launchers_use_relocated_python_only_for_private_venv(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            previous = os.environ.get("VIRTUAL_ENV")
            os.environ["VIRTUAL_ENV"] = str(root / "venv")
            try:
                for name, module in [("piper", "piper"), ("manim", "manim"), ("yt-dlp", "yt_dlp")]:
                    launcher = root / "venv/Scripts" / (name + ".exe")
                    launcher.parent.mkdir(parents=True, exist_ok=True)
                    launcher.write_bytes(b"stale distlib launcher")
                    launcher.chmod(0o755)
                    self.assertEqual(bridge.managed_render_command([str(launcher), "--help"]), [sys.executable, "-m", module, "--help"])
                    host = root / (name + ".exe")
                    host.write_bytes(b"host launcher")
                    host.chmod(0o755)
                    self.assertEqual(bridge.managed_render_command([str(host), "--help"]), [str(host), "--help"])
            finally:
                if previous is None:
                    os.environ.pop("VIRTUAL_ENV", None)
                else:
                    os.environ["VIRTUAL_ENV"] = previous

    def test_manim_uses_isolated_native_runtime_and_rejects_incomplete_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            prefix = root / "animation/env-test"
            python = prefix / "python.exe"
            manager = root / "micromamba.exe"
            prefix.mkdir(parents=True)
            python.write_bytes(b"private interpreter")
            manager.write_bytes(b"private manager")
            keys = ["JARVIS_OPENMONTAGE_MANIM_MANAGER", "JARVIS_OPENMONTAGE_MANIM_PREFIX", "JARVIS_OPENMONTAGE_MANIM_PYTHON"]
            previous = {key: os.environ.get(key) for key in keys}
            os.environ.update(dict(zip(keys, map(str, [manager, prefix, python]))))
            try:
                for name in ["manim", "manim.exe", str(prefix / "Scripts/manim.exe")]:
                    result = bridge.managed_render_command([name, "-ql", "scene.py", "Demo"])
                    self.assertEqual(result[0], str(manager))
                    self.assertEqual(result[-8:], [str(python), "-I", "-B", "-m", "manim", "-ql", "scene.py", "Demo"])
                    self.assertIn(str(prefix), result)
                # An explicitly selected external executable retains its authority.
                external = str(root / "manim.exe")
                self.assertEqual(bridge.managed_render_command([external, "--help"]), [external, "--help"])
                python.unlink()
                with self.assertRaises(bridge.BridgeError) as failure:
                    bridge.managed_render_command(["manim", "scene.py"])
                self.assertEqual(failure.exception.code, "renderer_unavailable")
            finally:
                for key, value in previous.items():
                    if value is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = value

    def test_catalog_metadata_does_not_load_native_sdks_but_details_verify_readiness(self):
        calls = []
        tool = SimpleNamespace(name="eye_enhance", capability="face_edit", provider="local",
                               runtime="local", stability="stable", dependencies=["mediapipe"],
                               capabilities=["face_edit"], side_effects=["filesystem"],
                               get_info=lambda: calls.append("checked") or {"status": "unavailable"},
                               check_dependencies=lambda: None)
        listed = bridge.tool_info(tool, check_dependencies=False)
        self.assertEqual(listed["name"], "eye_enhance")
        self.assertEqual(listed["dependencies"], ["mediapipe"])
        self.assertEqual(listed["status"], "not_checked")
        self.assertEqual(calls, [])
        self.assertEqual(bridge.tool_info(tool)["status"], "unavailable")
        self.assertEqual(calls, ["checked"])

    @unittest.skipUnless(os.environ.get("OPENMONTAGE_TEST_PACKAGE"), "Requires upstream package and test dependencies")
    def test_native_quote_handles_real_unpriced_elevenlabs_without_api_call(self):
        sys.path.insert(0, os.environ["OPENMONTAGE_TEST_PACKAGE"])
        from tools.audio.elevenlabs_tts import ElevenLabsTTS
        from tools.provider_pricing import PriceQuoteRequired
        tool = ElevenLabsTTS()
        inputs = {"model_id": "eleven_v4", "text": "Olá", "output_path": "voice.wav"}
        _, preflight = bridge.needs_approval(tool, inputs)
        self.assertEqual(preflight["cost_status"], "quote_required")
        with self.assertRaises(PriceQuoteRequired):
            tool.estimate_cost(inputs)
        quoted = bridge.quoted_preflight(tool, preflight, 0.75, True)
        self.assertEqual(quoted["cost_status"], "user_quote")
        self.assertEqual(tool.estimate_cost(inputs), 0.75)
        self.assertTrue(tool.dry_run(inputs)["estimated_cost_usd"] == 0.75)

    @unittest.skipUnless(os.environ.get("OPENMONTAGE_TEST_PACKAGE"), "Requires upstream package and test dependencies")
    def test_approved_hybrid_cannot_override_paid_tools_opt_out(self):
        sys.path.insert(0, os.environ["OPENMONTAGE_TEST_PACKAGE"])
        from tools.graphics.image_gen import ImageGen
        from tools.base_tool import ToolResult
        previous = {key:os.environ.get(key) for key in ["JARVIS_OPENMONTAGE_ALLOW_PAID", "OPENAI_API_KEY"]}
        original = ImageGen.execute
        called = []
        os.environ.update({"JARVIS_OPENMONTAGE_ALLOW_PAID":"0", "OPENAI_API_KEY":"test-no-real-api"})
        try:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                tool = ImageGen()
                tool._generate_openai = lambda inputs: called.append(True) or ToolResult(success=True)
                bridge.guard_nested_tools(None, True, root, root)
                with self.assertRaises(bridge.BridgeError) as failure:
                    tool.execute({"provider":"openai", "prompt":"No real call", "output_path":str(root / "image.png")})
                self.assertEqual(failure.exception.code, "paid_tools_disabled")
                self.assertEqual(called, [])
                bridge.require_paid_tools({"runtime":"hybrid", "estimated_cost_usd":0, "cost_status":"estimated"})
                with self.assertRaises(bridge.BridgeError):
                    bridge.require_paid_tools({"runtime":"hybrid", "estimated_cost_usd":None, "cost_status":"quote_required"})
        finally:
            ImageGen.execute = original
            for key,value in previous.items():
                if value is None:
                    os.environ.pop(key,None)
                else:
                    os.environ[key] = value

    @unittest.skipUnless(os.environ.get("OPENMONTAGE_TEST_PACKAGE"), "Requires upstream package and test dependencies")
    def test_nested_provider_and_weight_download_cannot_bypass_native_settings(self):
        sys.path.insert(0, os.environ["OPENMONTAGE_TEST_PACKAGE"])
        from tools.base_tool import BaseTool, ToolRuntime, ToolResult
        called = []
        class Provider(BaseTool):
            name = "test_nested_provider"
            runtime = ToolRuntime.API
            def execute(self, inputs):
                called.append(True)
                return ToolResult(success=True)
        class LocalSelector(BaseTool):
            name = "test_local_selector"
            def execute(self, inputs):
                return Provider().execute(inputs)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            bridge.guard_nested_tools(None, False, root, root)
            with self.assertRaises(bridge.BridgeError):
                LocalSelector().execute({})
            self.assertEqual(called, [])
            class ApprovedProvider(BaseTool):
                name = "test_approved_provider"
                runtime = ToolRuntime.API
                def execute(self, inputs):
                    called.append(True)
                    return ToolResult(success=True)
            previous = os.environ.get("JARVIS_OPENMONTAGE_ALLOW_PAID")
            os.environ["JARVIS_OPENMONTAGE_ALLOW_PAID"] = "0"
            bridge.guard_nested_tools(None, True, root, root)
            with self.assertRaises(bridge.BridgeError) as failure:
                ApprovedProvider().execute({})
            self.assertEqual(failure.exception.code, "paid_tools_disabled")
            self.assertEqual(called, [])
            if previous is None:
                os.environ.pop("JARVIS_OPENMONTAGE_ALLOW_PAID", None)
            else:
                os.environ["JARVIS_OPENMONTAGE_ALLOW_PAID"] = previous
        import requests.sessions
        import urllib.request
        original = requests.sessions.Session.request
        original_open, original_retrieve = urllib.request.urlopen, urllib.request.urlretrieve
        previous = os.environ.get("JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS")
        os.environ["JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS"] = "0"
        try:
            bridge.guard_model_downloads()
            with self.assertRaises(bridge.BridgeError) as failure:
                requests.get("https://huggingface.co/test/resolve/main/model.safetensors")
            self.assertEqual(failure.exception.code, "model_downloads_disabled")
            for download in [urllib.request.urlopen, urllib.request.urlretrieve]:
                with self.assertRaises(bridge.BridgeError) as failure:
                    download("https://github.com/example/weights/releases/model.pth")
                self.assertEqual(failure.exception.code, "model_downloads_disabled")
        finally:
            requests.sessions.Session.request = original
            urllib.request.urlopen, urllib.request.urlretrieve = original_open, original_retrieve
            if previous is None:
                os.environ.pop("JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS", None)
            else:
                os.environ["JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS"] = previous

    @unittest.skipUnless(os.environ.get("OPENMONTAGE_TEST_PACKAGE"), "Requires upstream package and test dependencies")
    def test_composer_template_writes_stay_in_production(self):
        sys.path.insert(0, os.environ["OPENMONTAGE_TEST_PACKAGE"])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            package = root / "installed"
            source = package / "remotion-composer"
            (source / "src").mkdir(parents=True)
            (source / "node_modules").mkdir()
            (source / "package.json").write_text("{}")
            (source / "src/index.tsx").write_text("immutable template")
            production = root / "production"
            production.mkdir()
            work = bridge.stage_composer(package, production)
            self.assertTrue(work.is_relative_to(production))
            staged = work / "remotion-composer"
            (staged / "src/index.tsx").write_text("working composition")
            self.assertEqual((source / "src/index.tsx").read_text(), "immutable template")
            self.assertEqual((staged / "node_modules").resolve(), (source / "node_modules").resolve())
            from tools.video.remotion_caption_burn import RemotionCaptionBurn
            self.assertEqual(RemotionCaptionBurn()._find_remotion_root(), staged)

    @unittest.skipUnless(os.environ.get("OPENMONTAGE_TEST_PACKAGE") and shutil.which("ffmpeg") and shutil.which("ffprobe"), "Set OPENMONTAGE_TEST_PACKAGE and install test dependencies for upstream integration")
    def test_real_registry_pipeline_tool_review_and_failures(self):
        package = Path(os.environ["OPENMONTAGE_TEST_PACKAGE"]).resolve()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            production = root / "production"
            def run(action, **kwargs):
                request = {"root": str(root), "directory": str(production), "package": str(package),
                           "action": action, "nativeApproved": False, **kwargs}
                request_path = root / "request.json"
                request_path.write_text(json.dumps(request), encoding="utf-8")
                environment = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1", "OPENMONTAGE_CONFIG": str(root / "missing-config.yaml")}
                result = subprocess.run([sys.executable, str(BRIDGE), "--request", str(request_path)], capture_output=True, text=True, env=environment, timeout=90)
                response = next(json.loads(line) for line in reversed(result.stdout.splitlines()) if line.startswith("{"))
                return result.returncode, response
            code, initialized = run("init", pipeline="screen-demo", arguments={"title": "Real integration"})
            self.assertEqual(code, 0, initialized)
            self.assertEqual(initialized["project"]["pipeline_type"], "screen-demo")
            code, listed = run("tools")
            self.assertEqual(code, 0, listed)
            self.assertGreater(listed["total"], 70)
            self.assertIn("audio", " ".join(listed["capabilities"]))
            code, info = run("tools", tool="video_compose")
            self.assertEqual(code, 0, info)
            self.assertIn("input_schema", info["tool"])
            code, unknown = run("tool", tool="invented_tool", arguments={})
            self.assertNotEqual(code, 0)
            self.assertEqual(unknown["error"]["code"], "unknown_tool")
            code, bypass = run("approve", arguments={"stage": "idea", "approved": True})
            self.assertNotEqual(code, 0)
            self.assertEqual(bypass["error"]["code"], "approval_required")
            code, escape = run("preflight", tool="video_stitch", arguments={"clips": [str(root.parent / "outside.mp4")], "output_path": "stitched.mp4"})
            self.assertNotEqual(code, 0)
            self.assertEqual(escape["error"]["code"], "project_scope")
            code, escape = run("preflight", tool="video_compose", arguments={"operation": "encode", "input_path": "source.mp4", "output_path": "../other-assignment/final.mp4"})
            self.assertNotEqual(code, 0)
            self.assertEqual(escape["error"]["code"], "project_scope")
            source = production / "source.mp4"
            subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=blue:s=160x90:d=0.5", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(source)], check=True)
            code, rendered = run("tool", tool="video_compose", arguments={"operation": "encode", "input_path": "source.mp4", "output_path": "next.mp4"})
            self.assertEqual(code, 0, rendered)
            self.assertTrue(rendered["videos"][0]["verified"])
            self.assertEqual(rendered["videos"][0]["path"], "production/next.mp4")
            self.assertGreater(rendered["videos"][0]["probe"]["streams"][0]["width"], 0)
            self.assertTrue((production / "cost_log.json").is_file())
            code, previous = run("tool", tool="video_compose", arguments={"operation": "encode", "input_path": "source.mp4", "output_path": "next.mp4"})
            self.assertNotEqual(code, 0)
            self.assertEqual(previous["error"]["code"], "output_exists")
            code, unavailable = run("tool", tool="cartesia_tts", arguments={"text": "Olá", "voice_id": "missing", "output_path": "voice.wav"})
            self.assertNotEqual(code, 0)
            self.assertIn(unavailable["error"]["code"], {"tool_unavailable", "openmontage_error"})
            source.write_bytes(b"not a real MP4")
            code, invalid = run("review", arguments={"output_path": "source.mp4"})
            self.assertNotEqual(code, 0)
            self.assertEqual(invalid["error"]["code"], "invalid_video")


if __name__ == "__main__":
    unittest.main()
