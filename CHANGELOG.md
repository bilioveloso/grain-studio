# Changelog

## 1.1.0

- The program file is `Grain Studio.exe`, no longer `RapidRAW.exe`.
- RapidRAW's cloud sign-in, which Grain Studio never offered, is no longer loaded: nothing contacts a sign-in
  service when the app starts.
- **iPhone edits come along.** A photo edited in the iPhone's Photos app shows an "iPhone edit" in its versions:
  one click puts that edit on the sliders as a starting point, the eye opens Apple's own render (IMG_E1234) as
  it is. Lightroom ignores both. Light, colour, white balance, definition, vignette, sharpening, noise reduction,
  the Vivid / Dramatic / Silverplate filters and the crop all come over, mapped by a fit on 19 real iPhone edits
  against Apple's own renders. Colour still runs a little richer than Apple's. Photographic Styles, Clean Up,
  Portrait depth and straightened crops aren't translated.
- **ProRAW opens at the right brightness.** iPhone ProRAW keeps up to 5 EV of headroom and says how bright to
  show it (BaselineExposure); Grain Studio ignored that, so ProRAWs opened dark. An unedited ProRAW now opens
  where Apple's own render sits. Photos already edited keep their exposure; their thumbnails stay as they were
  until edited.
- **Import a Lightroom Classic catalog** (command palette). Ratings, picks and rejects, colour labels, keywords
  and collections come over as tags, and each photo's develop settings as a "Lightroom" version. Nothing you
  did in Grain Studio is overwritten, the catalog is read from a private copy and never written, and a catalog
  Lightroom has open is refused. Virtual copies, crops and local masks don't come over yet.
- **ProRAW sky masks use the iPhone's own sky matte**: instant, and they keep the cables and branches the sky
  model loses.
- Edits, versions and settings are written atomically: a crash mid-save can't leave half a sidecar.
- **Culling finds closed eyes.** Turn on Find Closed Eyes and frames where anyone blinked get their own tab,
  pre-selected to reject -- and a frame with closed eyes is never a burst's pick while any frame has everyone's
  eyes open. Needs the small Faces models (5 MB, Settings -> AI Models), fetched the first time.
- **Fast bursts stay together.** A burst that pans with its subject -- a bird taking off at 10 fps -- was split
  into single frames because the background kept changing. Frames under a second apart now join unless the
  scene itself changed.
- **Lens blur works when the assistant turns it on.** It needs a depth map, which only the Effects panel made.
- **Lightroom presets made on JPEGs keep their white balance** (IncrementalTemperature / IncrementalTint were
  dropped on import).
- The assistant can cull photos too (cull_photos).

## 1.0.0

The first release. Grain Studio is a photo editor with an assistant that edits inside real masks and real
sliders — everything it does, you can see, move and undo. It never repaints pixels.

### Talking to the photo

- **Mark a place** on the photo and say what should change there. The mark is a target, not a hint: the app
  builds a mask on it and changes only what is inside.
- **Circle an area** roughly and it finds the thing inside your circle — the crane, not the circle around it.
  The ring keeps the subject model honest; the subject model keeps the ring from being a blunt shape.
- When the model's cut is not an answer — the whole box, or a speck in one corner — your drawn shape is used as
  drawn, rather than a mask that edits almost nothing.
- Marks can be nudged: drag the numbered badge and the mark follows, a circle bringing its shape with it.
- **Plain edits cost nothing.** "darker", "a touch warmer", "punchier" and a dozen more are slider moves the app
  makes itself — no tokens, no wait, the same answer every time. Anything with a condition or a comparison in it
  goes to the assistant instead, which is the safe direction to fail in.
- **The assistant answers in numbers** where a number will do, and spends a picture only when the look or a
  mask's shape is the actual question. A circled edit costs about 95k tokens where it once cost 420–496k.
- Choose who answers: a model on your own computer, Claude, both, or no assistant at all.

### Masks

- **Subject masks sit on the photograph's own edge.** The subject finder now looks twice -- once at the whole
  photo, then again at a crop around what it found, so its grid is up to three times finer -- and a colour
  matte decides each edge pixel from the photo itself. Sky seen through a crane's lattice stays sky; on the
  harbour crane, spill past the steel fell from 72% to 13%.
- **Fine and Finest subject edges** (optional, Settings -> AI Models, 27 MB or 104 MB): a matting model
  trained on hair, fur and feathers decides the edge where colour cannot -- a white bird on an overcast sky.
  Finest is used when both are installed.
- Refine Edge and Contrast on AI masks, so a selection lands on the photo's own edges.
- New sky and foreground masks come with a little Contrast, because those models run small and their output
  is a wide soft ramp by the time it reaches a real photo — that ramp is the halo people see. Subject masks
  start at zero: Refine and Contrast would only blur and crush a matte that is already on the edge.
- Repairing a mask is one click: **Paint in what it missed** and **Paint away the spill**.
- Asking the assistant for the sky uses the dedicated sky model, which knows what sky is between branches.

### Editing

- **Heal finds its own source.** Brush over a blemish; it looks for the patch of photograph whose surroundings
  match, and borrows from there. Ctrl+click still overrides. Clone still asks you to point.
- **Export a cutout**: pick a mask under File Settings and the photo comes out as a PNG with everything outside
  it transparent.
- Shift + and Shift − step through the double-exposure blend modes.
- Zoom goes to eight times the photo's own pixels, with a real pixel grid past twice actual size.

### First run

- One screen reads the computer and says in plain words what it can run, then sets it up — a model on this
  machine, Claude, both, or nothing.

### Known limits

- **Sky and foreground mask edges are not feather-accurate.** Those masks come from 320 px models, and Refine
  Edge smooths rather than mattes, so trees against sky can show a soft edge. Subject masks no longer have this
  limit. A subject mask on a large subject takes longer to make (about 15 s for a bird filling the frame with
  Finest edges installed).
- **Neither installer is code-signed**, so Windows shows "protected your PC" and macOS quarantines the app.
- **Translations lag the English strings.** Everything added since 0.9.0 falls back to English.
- The local-model path needs Ollama installed separately.
