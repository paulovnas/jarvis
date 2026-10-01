# Managed ComfyUI image workflows

Jarvis runs the official ComfyUI executor in a private CPU-only Python process.
It does not start an HTTP server or load custom-node directories. The fixed
graph uses official LoadImage, ImageScale and SaveImage nodes, with two trusted
Jarvis adapters for alpha preservation and local background removal.

- ComfyUI v0.3.8, revision `9f4b181ab38b246961c5a51994a8357e62634de1`.
  Source: https://github.com/Comfy-Org/ComfyUI/tree/9f4b181ab38b246961c5a51994a8357e62634de1
  Source archive SHA256: `a5ccb341db71f1af757a18468b7bab1f823689cbc1fd242c211e22f07557c1a2`.
  GPL-3.0-or-later; the entire upstream source and LICENSE accompany the private runtime.
  This compatibility pin supports Torch 2.2.2 on Intel Macs.
- Python 3.11.16: Astral python-build-standalone 20260929, platform checksums
  shared with the Audiovisual installer. Each component has independent Python
  and site-packages. Only immutable download/wheel caches may be shared.
- Background removal: ONNX Runtime 1.20.1 (MIT),
  U2-Net `u2netp.onnx`, Apache-2.0.
  https://github.com/xuebinqin/U-2-Net
  https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx
  Model SHA256: `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`.

All model downloads happen during Core installation. Runtime jobs use the
checked local model through a fixed offline session. The preprocessing follows
the U2-Net contract used by rembg; unused OpenCV, alpha-matting and JIT
libraries are not imported or installed.
Image generation/editing remains the user's configured image provider. This
component prepares those images; it does not provide paid Comfy partner APIs,
diffusion checkpoints, ControlNet or LoRA. Upscaling uses interpolation and
does not synthesize new detail.
