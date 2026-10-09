# Grain Studio

**A photo editor for real photographs, with an assistant that edits alongside you.**

Grain Studio pairs a quiet, Lightroom-style workspace with an assistant that can see your photo, move the
sliders, build masks and create looks while you watch. Every step shows up on the sliders and can be undone.
Viral effects like fractal glass, silky water and light trails are built from your real pixels, not generated.

Built on the [RapidRAW](https://github.com/CyberTimon/RapidRAW) engine by Timon Käch.

## What makes it different

### An assistant in the editor
- Ask in plain words ("warm up the light, keep the sky honest") and watch it edit.
- **Mark a place** on the photo, or **circle something**, and say what should change there. Unlike a chat that
  repaints your picture, a mark becomes a real mask: the marked area changes and the rest of the photo keeps its
  own pixels. Circling is rough on purpose: the subject-detection model cuts out what is inside your ring and the
  ring keeps that cut from wandering, so a loose circle around a crane masks the crane, not the sky between its
  struts. It runs on your computer, so the assistant spends its turn on the edit rather than on finding the thing.
- Plain asks ("darker", "a touch warmer", "punchier") are done by the app itself, at once and for nothing; the
  assistant is called when the request has a condition or an idea in it.
- It looks at the result before answering, and keeps shadows, vignette and clarity in check.
- Runs on your **Claude** subscription, or on a **local model** (Ollama, LM Studio) that stays on your computer,
  or both at once, with a switch in the chat bar saying who answers the next message.
- On a fresh install, one screen looks at your computer, says what it can run, and sets it up for you.
- Choose the model in Settings (Sonnet by default, Opus for harder jobs), and see what a conversation has used:
  the panel shows the tokens and what they would cost on the API, which your subscription covers.

### Looks in one click
- **Ten ready-made looks**, each built from real adjustments and effects and saved as a version you can flip back from.
- **Match a look:** pick a reference photo, and the assistant recreates its colour, light and effects on yours.

### Creative effects
- **Fractal glass** with straight or curved ribs, **pixel stretch** and **reflection**.
- **Motion:** movement blur, silky water and light trails.
- **Double exposure** with blend modes and drag-to-place, **frames** in any colour, and **3D pop-out bars** the subject breaks out of.
- Place effects on the photo with draggable guides, and limit any of them to a mask.

### Masks that understand the photo
- **Smart masks:** the assistant marks water, sky, buildings, people, hair or skin; a local segmentation model draws
  the mask, and it checks the overlay and fixes spills.
- **Point** and **Quick Select** masks for fast local edits.
- **Refine Edge** snaps sky and subject masks to the photo's real edges (hair, branches, rooflines), and **Contrast**
  clears the grey haze; the assistant can use both.

### Versions
- Named versions, automatic history, and your original always kept as the first version.
- **Your iPhone edits come along:** a photo edited in the iPhone's Photos app shows an *iPhone edit* version, the
  edit already on real sliders (light, colour, filters, crop), with Apple's own render one click away.

### Coming from Lightroom or an iPhone
- **Import a Lightroom Classic catalog** (command palette): ratings, picks, colour labels, keywords, collections and
  each photo's develop settings as a "Lightroom" version. The catalog is only ever read from a private copy.
- **iPhone ProRAW** opens at the brightness Apple shows it, and sky masks use the phone's own sky matte.

### A professional workflow
- **Cull with flags:** P picks, X rejects, U clears; filter the library to picks or rejects.
- **Culling finds bursts, blur and closed eyes**, and picks the frame where everyone's eyes are open.
- **Before / after** side by side with Y, and a **History** list to jump back to any step.
- **Sync** chosen settings to many photos in one step.
- **Edit in Photoshop** (or any editor): a 16-bit TIFF next to the original, back in your library.
- **Auto Upright** for converging verticals, and **local effects** (dodge, burn, soften skin...) to paint on.
- Exports embed an sRGB colour profile, so labs and other apps read the colours right.

### Finished photos, bigger
- **Upscale** exported photos to 4K: Classic, or AI (Real-ESRGAN on your graphics card).
- Or let the assistant upscale both ways, compare the detail and tell you which to keep.

### Clean Mode
On by default: real photographs only. AI-assisted erase, heal and clone work as in any phone editor; generative AI that
changes the scene (prompt-based generative fill) and AI upscaling stay off until you choose otherwise.

### Export
- Instagram presets, output sharpening, true 16-bit TIFF, and Lightroom Mobile presets (.xmp).
- Command-line export for batch work.

## From the RapidRAW engine

The RAW engine, the GPU processing pipeline and the core editing tools come from RapidRAW: fast,
non-destructive editing of RAW, ProRAW (including iPhone DNG), JPEG, HEIC and TIFF files.
Grain Studio follows RapidRAW's updates, so engine improvements keep arriving.

## Getting Grain Studio

Grain Studio is in active development. Current version: **1.1.0**.

### Install (Windows and Mac)

Ready-built installers for Windows and Mac are not distributed from this repository. They will be offered at
[grainhaus.eu](https://grainhaus.eu/grain-studio). The source here is complete: anyone may build the app themselves,
as described below.

### Build and run from source

You need [Node.js 22](https://nodejs.org/), [Rust](https://rustup.rs/) and Git, plus the system tools for your OS:

| OS | Also install |
| --- | --- |
| Windows 10/11 | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with *Desktop development with C++*. Python 3 for the GPU runtime (optional). |
| macOS | Xcode command line tools: `xcode-select --install` |
| Linux (Ubuntu/Debian) | `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config` |

Then, on any OS:

```bash
git clone https://github.com/bilioveloso/grain-studio.git
cd grain-studio
npm install
npm start
```

The first build takes a while (Rust compiles everything once); later starts are fast.

On Windows, run this once before `npm start` so AI features use the graphics card instead of the CPU:

```bash
python scripts/fetch-directml-runtime.py
```

To build an installer on your own machine:

```bash
npm run tauri build
```

The result lands in `src-tauri/target/release/bundle/`: `nsis/` (.exe) on Windows, `dmg/` on Mac, and
`deb/`, `rpm/` and `appimage/` on Linux.

## Credits and license

Grain Studio is built on [RapidRAW](https://github.com/CyberTimon/RapidRAW) by Timon Käch. Thank you for
an excellent open-source engine. Like RapidRAW, Grain Studio is licensed under the **AGPL-3.0**;
see [LICENSE](LICENSE). RapidRAW's original README is kept as [README-RapidRAW.md](README-RapidRAW.md).
