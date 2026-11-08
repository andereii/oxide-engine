# RENDER-BEHAVIOR — M5 phase-2b visual slice (observed semantics for stage/character/strumline rendering)

Provenance: compiled 2026-11-07 from read-only observation of the reference checkout
`/home/alex/Descargas/fnf/Funkin/` (source/, .haxelib/, assets/). Two analyst rounds
(character/atlas; stage/camera/strumline) plus orchestrator spot-checks (headless render
probe, flxanimate internals). Every fact is labeled **OBSERVED** (file+line in the
reference) or **DERIVED** (direct consequence of observed code, including transcribed data
values). "Engine decision" items are our choices for the Rust reimplementation and are NOT
observed behavior. This document is the behavior specification for M5 phase 2b;
`docs/M5-CONTRACT.md` §7 corrigenda continue to apply. Nothing here proves native-game
pixel parity.

Scope: stage mainStage, characters bf/dad/gf (default atlas content), note style `funkin`,
upscroll, 1280x720 game size, no practice/botplay, no story/HUD text. OUT of the phase-2b
frames: judgement popups, health bar, HUD text, cutscenes, story UI, bf
death/fakeout animations. ALSO EXCLUDED by explicit engine scope decision (2026-11-07
aggregate review): chart song events (`FocusCamera` camera-focus swaps and `PlayAnimation`
`hey` plays native performs on bopeebo) — the phase-2b renderer keeps dad's initial focus
for the whole song and does not play chart-driven character animations; and hold covers /
note splashes (zIndex 40/50 layers). These are deliberate exclusions with stated parity
consequences (ADR-0020 addendum 3), NOT claims that native matches this simplification.

---

## 1. Scene assembly: stage props and character slots

OBSERVED (`source/funkin/play/stage/Stage.hx`):

- Prop creation (L170-199): `isSolidColor` = assetPath starts with `#`; a prop becomes a
  Bopper only when `animations.length > 0 || danceEvery != 0`; otherwise it is a plain
  `StageProp`. mainStage.json defines stageBack/stageFront/stageCurtains with empty
  `animations: []` and `danceEvery: 0` → all three are plain static sprites despite
  `animType: "sparrow"`; the `switch animType` still runs `addAtlasAnimations(sprite, [])`
  (L269-292) — a no-op. Behaviorally equivalent: static textured sprite.
- Asset resolution: `loadTexture` → `Paths.image(key)` (`FunkinSprite.hx:236-239`,
  `Paths.hx:162-165`) → `assets/shared/images/stageback.png`, `stagefront.png`,
  `stagecurtains.png` (files exist).
- Prop parameter application (Stage.hx L234-267): `scale` (scalar Left or `[sx,sy]` Right);
  `updateHitbox()`; `x = position[0]`, `y = position[1]`; `alpha`; `antialiasing = !isPixel`
  (true here); `scrollFactor = scroll`; `angle`, `color`, `blend` (none here);
  `zIndex = dataProp.zIndex`; `flipX/flipY` from data.
- Grouping/sorting: Stage is a FlxSpriteGroup; `refresh()` sorts members by zIndex
  ASCENDING (Stage.hx L358-361) and PlayState calls `currentStage.refresh()` after
  characters are added (PlayState.hx L2234-2236). Group draw order = member array order
  after that sort (there is no automatic zIndex sort in these flixel classes — verified by
  absence otherwise).
- Character slots (Stage.addCharacter L400-498):
  - `character.zIndex = stageCharData.zIndex` (L449).
  - Feet-anchored position: `x = pos[0] − origin.x`, `y = pos[1] − origin.y` (L455-456)
    with `characterOrigin = (width/2, height)` (BaseCharacter.hx L88-95); slot positions
    for bopeebo are the M5-CONTRACT §0 values.
  - `finalScale = getBaseScale() * stageCharData.scale` (L458; slot scale default 1.0);
    `setScale` re-pins the feet position (setScale L277-288).
    When stageCharData.scale == 1, character scale = data scale (1.0 here).
  - Camera focus: additive combination —
    `resetCameraFocusPoint()` = `(originalPos.x + width/2 + charJSON.cameraOffsets[0],
    originalPos.y + height/2 + charJSON.cameraOffsets[1])` (BaseCharacter.hx L350-356,
    char JSON offsets), then stage-slot offsets are added
    (`+= stageCharData.cameraOffsets`, Stage.hx L465-466).
  - `scrollFactor = slot.scroll ?? (1,1)` (L468-469; bopeebo slots define no scroll).
- PlayState top-level insert order (create(), PlayState.hx L886-898): cameras →
  healthBar → stage (props+characters) → strumlines → popups. HUD renders above the game
  because `FlxG.cameras.reset(camGame)` then camHUD is added second
  (PlayState.hx L1969-1970) — camera list order, not zIndex.

## 2. Camera

- Game size: `Main.hx:33-34` = 1280x720 game pixels. FullScreenScaleMode
  (`funkin/ui/FullScreenScaleMode.hx:319-336`) refills the main camera to 16:9 (desktop:
  no letterbox). `camCutouts` covers the full 1280x720 (PlayState.hx L1962-1964).
  **Engine decision**: render_diff/perf_budget render at exactly 1280x720 with scale
  factor 1 (physical window == logical game pixels).
- Initial focus: an invisible `cameraFollowPoint` object positioned at **dad's** combined
  cameraFocusPoint during character init (PlayState.hx L2226-2227); follow:
  `FlxG.camera.follow(point, LOCKON, Constants.DEFAULT_CAMERA_FOLLOW_RATE)` (L3926),
  `followLerp = 0.04` (Constants.hx L696); `focusOn(point)` snaps the initial scroll
  (L3937): `scroll = point − (width/2, height/2)`.
- Zoom: `currentCameraZoom = stageZoom = stage.cameraZoom ?? FlxCamera.defaultZoom*1.05`
  (PlayState.hx L323-330; `defaultZoom = 1.0`); mainStage `cameraZoom = 1.1`.
  On stage load: `FlxG.camera.zoom = currentCameraZoom; cameraBopMultiplier = 1.0`
  (L2098-2107).
- Beat zoom pulse (OBSERVED PlayState.hx L1846-1859 + decay L1217-1228):
  - Gate on each step hit: `Preferences.zoomCamera && camHUD.zoom < 1.35*defaultHUDCameraZoom
    && cameraZoomRate > 0 && (currentStep + cameraZoomRateOffset*4) % (cameraZoomRate*4) == 0`
    with `cameraZoomRate = 4` (Constants DEFAULT_ZOOM_RATE L246) and
    `cameraZoomRateOffset = 1` (PlayState.hx L364 default) — DERIVED: fires every 4 beats;
    on that step `cameraBopMultiplier = DEFAULT_BOP_INTENSITY = 1.015` (Constants.hx
    L241); camHUD zoom += `(cameraBopIntensity − 1.0)*2.0` (= 0.03; reset at L1109-1110).
  - Per-frame decay (L1217-1228; `dt = elapsed*60`):
    `cameraBopMultiplier = lerp(1.0, cameraBopMultiplier, pow(0.95, dt))`;
    `FlxG.camera.zoom = currentCameraZoom * cameraBopMultiplier`.
- Projection (DERIVED from FlxCamera math with `initialZoom = 1` and scaleMode scale 1):
  ```
  screen.x = 640 + zoom * (world.x − camera.scroll.x*scrollFactor.x − 640)
  screen.y = 360 + zoom * (world.y − camera.scroll.y*scrollFactor.y − 360)
  ```
- Follow lerp per frame (OBSERVED FlxCamera.hx L1199-1291):
  `scrollTarget = target.midpoint + targetOffset − (width/2, height/2)`;
  `scroll += (scrollTarget − scroll) * (1 − (1 − followLerp)^(elapsed*60))`.

## 3. Strumline rendering (note style `funkin`)

Constants (Strumline.hx L34-47, Constants.hx): `STRUMLINE_SIZE = 104`,
`NOTE_SPACING = 112`, `INITIAL_OFFSET = −0.275*104 = −28.6` (upscroll), `NUDGE = 2.0`,
`KEY_COUNT = 4`, `PIXELS_PER_MS = 0.45`, `STEPS_PER_BEAT = 4`, and
`renderDistanceMs = 720 / 0.45 / (scrollSpeed < 1 ? scrollSpeed : 1)` = 1600 ms for the
locked slice config (speeds 1.2/1.3/1.6).

Structure (OBSERVED):

- Lines: player `x = 688`, opponent `x = 48` (PlayState.hx L2249-2264,
  `Constants.STRUMLINE_X_OFFSET = 48`), both `y = 24` (`STRUMLINE_Y_OFFSET = 24`);
  desktop cutout size = 0.
- Receptors (StrumlineNote, 4 per line): `x = getXPos(dir) + INITIAL_OFFSET` (L267-268)
  with `getXPos = dir * NOTE_SPACING` (L1333-1352); `y` = noteStrumline y-offset [0,0];
  scale 0.7; `updateHitbox`; `centerOrigin/centerOffsets`; animations from prefixes
  `staticX0/pressX0/confirmX0` (funkin.json).
  CONTENT placement (the flx matrix path; corrected 2026-11-08 — the wrong
  first derivation and the later +13 term were both caught by review and
  re-verified against the exact source): the RENDERED content top-left (game
  pixels) =
  `spriteTopLeft + s·dest_offset`
  — i.e. the frame's `dest_offset` (= −(frameX, frameY) from the Sparrow XML,
  our atlas-crate contract) scaled ONCE, for EVERY receptor state and notes;
  there is NO glow/confirm compensation term (see the superseded-history
  discussion below this paragraph). Derivation with
  exact citations: origin = UNSCALED frame center (`centerOrigin` =
  `origin.set(frameWidth·0.5, frameHeight·0.5)`, FlxSprite.hx:995-998);
  the draw chain is `tile → translate(−origin) → scale(s) → translate(pc − offset +
  origin)` (`drawFrameComplex`, FlxSprite.hx:876-905), and OpenFL's
  `Matrix.scale` SCALES the accumulated `tx/ty` as well
  (`openfl/geom/Matrix.hx`: function scale — `tx *= sx; ty *= sy`), and the
  frame tile matrix carries `translate(frame.offset(gpu path))` with
  `offset = −(our dest_offset)` for Sparrow (FlxFrame.copyFrom forBlit=false;
  FlxAtlasFrames.fromSparrow). The `origin` and `offset` terms cancel
  exactly, leaving `spriteTopLeft + s·dest_offset` for EVERY receptor state
  AND notes. The −13 confirm compensation lives ONLY inside
  `StrumlineNote.fixOffsets()` (StrumlineNote.hx:178-198) which is DEAD CODE
  in this revision: it has no call site (grep over source/ shows only the
  definition; the actual `playAnimation`/`setup`/`update` paths call
  `centerOffsets(); centerOrigin()`). The requested "minus 13 px for
  confirm" from the playtest round was therefore an assumption, falsified
  by reachability — no glow term exists (an earlier formula with
  `−sourceSize·(1−s)²/2` and a wrong −13 direction are superseded; an
  untrimmed note frame/frameX=0 draws exactly at its sprite position).
- Assets from `preload/data/notestyles/funkin.json`: `shared:notes` (noteLeft…noteRight,
  scale 0.7), `shared:noteStrumline` (scale 0.7, offsets [0,0]),
  `NOTE_hold_assets` (scale 0.7); sparrows resolve to
  `assets/shared/images/{notes,noteStrumline}.{xml,png}` (files exist).
- z-order INSIDE one strumline (L211-226 + refresh L308-311): background (0) → receptors
  (10) → holds (20) → notes (30) → hold covers (40) → splashes (50). Background = solid
  black rect: width `4*112 + 2*16 = 480`, height = FlxG.height, `x = −16` (respect to the
  strumline origin), `alpha = fullscreenOpacity/100` default, `scrollFactor (0,0)`.
- Note sprite while scrolling (L614-617): `y = this.y − INITIAL_OFFSET +
  getNoteY(strumTime, scrollSpeed, isDownscroll, conductor)` where
  `getNoteY = 0.45 * (songPosition + songPositionDelta − strumTime) * scrollSpeed *
  (down ? 1 : −1)` (GRhythmUtil.hx L119-129).
- Note x at spawn (L1168-1172): `x = this.x + getXPos(dir) − (sprite.width − 104)/2 −
  NUDGE`; x is NOT recomputed per frame afterwards.
- SustainTrail (SustainTrail.hx L209-410): texture NOTE_hold_assets 416×87; columns 8;
  `graphicWidth = (graphic.width / 8) * zoom = (416/8) * 0.7 = 36.4` Dx (the 52 figure
  was the pre-zoom column width and was a transcription error — corrected 2026-11-07 by
  review); height = `0.45 * sustainLengthMs * speed`
  (`sustainHeight`), clipHeight = remaining from the current song ms; strip = the middle
  column for the direction, end cap `87*0.7*0.5 = 30.45` px tall with
  `bottomClip = 0.9` UV bound; x = `this.x + getXPos(dir) + 104/2 − 52/2`;
  y while held = `this.y − INITIAL_OFFSET + 104/2` FIXED (764-768); a dropped hold adds
  `yOffset = (fullFullLen − currentLen)*0.45` (L690-710). Trails render BEHIND notes and
  ABOVE receptors. DROPPED holds: the reference sets the trail's alpha to 0.0
  ("Completely hide the dropped hold note", Strumline.hx:634-640) — dropped hold trails
  are HIDDEN, not drawn (supersedes any reading where the yOffset implies a visible
  dropped trail; corrected by review, 2026-11-07).
- Receptor animation triggers: press/confirm per input state; confirm-hold for holds;
  opponent confirm auto-resets after 0.15 s (StrumlineNote.hx confirmHoldTimer L92-98).
  At song start every receptor plays `static`; `fadeInArrows()` tweens each arrow
  `y −= 10→y+10, alpha 0→1` over 1.0 s with `0.2*i` delay (L1354-1391, expo easing).
  **Engine decision**: render_diff sample frames start at/after the fade completes, so
  neither the tween nor randomness attaches to frozen digests (the fade itself is not
  part of the frozen frames).

## 4. Character atlases (BTA format) and frame content

Parsology (OBSERVED in `.haxelib/flixel-animate/git/src/`, ref `c5e3393c…`):

- File shape: `{AN, SD, MD}`; `MD = {V, N, BGC, W:1920, H:1080, ASV:3, FRT:24}` —
  timeline frame rate = 24 fps; stage rect = MD W/H.
- `AN` = main symbol instance block `{N, SN, TL:{L:[layers]}}`; layer `L[i]` =
  `{LN, FR:[keyframes {N?(label), I(start), DU(duration), E:[elements]}]}`;
  keyframes expand each to timeline frames I..I+DU−1 (Layer.hx L180-191).
  Timeline frameCount = max over layers.
- `SD.S` = ARRAY of symbol timelines `{SN(name), TL:{...}}`; symbols are looked up by
  name from this dictionary, and (multi-atlas) from merged collections after the main.
  Label comparison is the OBSERVED `name.rtrim() == label` (FlxAnimateController.hx
  L266-290, verbatim; no case folding, no left trim; RIGHT trim only, both sides).
- Elements: `SI` = symbol instance `{SN, FF (first-frame offset), ST ("G"|"MC"…),
  TRP (transformation point — INERT for rendering), LP ("LP" loop), MX:[a,b,c,d,tx,ty]}`,
  `ASI` = atlas sprite instance `{N (frame digit name), MX}`.
- Draw semantics:
  - Timeline.draw iterates layers DESC (last index first); elements within a frame ASC
    (first behind later). Bitmaps render only via ASI.
  - ASI quad: bitmap rect = spritemap entry `(x,y,w,h)` (the (x,y) is UV-only);
    the drawn quad is size (w,h) with top-left at the matrix-transformed local (0,0).
  - Matrix chains (concat semantics `m.concat(parent)` = "apply local first, then
    parent"): accumulated world matrix of any bitmap =
    `tileMX(spritemap frame) · MX_asi · MX_si_leafchain… · MX_main_si · spriteMatrix`,
    innermost applied first; `tileMX` is identity unless the spritemap entry is
    `rotated` (dad sprites "2","3","6","11" are rotated → −90°).
  - SI recursion into symbol timelines: displayed child frame =
    `FF + (t − keyframe.I)`, then LOOP (`LP`) wraps mod frameCount, PLAY_ONCE (`PO`)
    clamps to last frame, SINGLE_FRAME (`SF`) frozen at FF. `ST:"G"` changes nothing for
    the math; `"MC"` symbols play their child timelines when `swfMode = true` (fun
    inventory: `DEFAULT_ATLASSETTINGS.swfMode = true` — bf uses MCs inside its death
    anims; the main atlas IDs bf/dad/gf exhibit only `G` symbols per this checkout).
  - Per-quad `ColorTransform`s (`C`) appear in bf-death only; bf/dad/gf main timelines
    have none.
  - Bounds: character sprite content rect `bounds` = union of transformed quads
    (Timeline.getWholeBounds semantics; ~`(−194.9, −333.65, 748.8, 674.85)` for gf,
    `(−270.15, −582.4, 566.8, 860.0)` for dad, `(−214.4, −271.3, 496.9, 513.9)` for bf —
    approximate ±0.1, derive exactly at implementation time from the real data).
  - Whole-sprite draw composition (FlxSprite matrix chain with `origin = (frameW/2,
    frameH/2)` and updateHitbox offsets 0 at scale 1) collapses so that:
    **`gameQuad(worldPoint) = spriteTopLeft + (worldPoint − boundsTopLeft)`** where
    `spriteTopLeft = screenPos − offset` and Bopper adds the data-driven position shift
    `−(animOffsets − globalOffsets) * scale` (Bopper.hx L335-341). bf/dad/gf per-anim
    offsets are [0,0] for the non-death animations, so the observable shift is 0 there.
- Main `AN.TL` labels (OBSERVED data, frame spans `I/DU`):
  - bf Layer_3 labels: Idle 0/14, Left 14/8, "Left Miss" 22/34, Right 56/7,
    "Right Miss" 63/46, Up 109/7, "Up Miss" 116/24, Down 140/7, "Down Miss" 147/29,
    Hey 176/26, Cheer 202/9, Scared 211/12.
  - gf Layer_2 labels: Idle 0/30, Up 30/7, Down 37/20, Left 57/15, Right 72/15,
    Cheer 87/21, Crying 108/12, Fear 120/4, "Idle (Hair blowing)" 124/30,
    "Hair landing" 154/30.
  - dad Layer_2 labels (observed directly in the file): Idle 0/12, Up 12/6, Down 18/6,
    Left 24/6, Right 30/6; content Layer_1 has the matching 5 keyframes (one element
    each). So `idle` spans frames 0-11 and `Idle-hold` indices [11,12,0,1] resolve to
    timeline frames [11, 0, 1] (12 is out of the Idle span 0..11 and is skipped).
- Multi-atlas (bf): main atlas + sub-atlases for the death-family anims only
  (`bf-death` ×3, `bfFakeOut` ×1); label/symbol lookup searches the main timeline FIRST,
  then merged collections. **Engine decision**: load only the atlases referenced by
  character data; combine order = main first, then data animation order; death/fakeout
  animation content is NOT part of phase-2b frames (`game_over` is a slice boundary).
- Data quirk (DERIVED, re-verify at implementation): `addByFrameLabelIndices` skips
  out-of-range indices — e.g. dad `idle-hold` frameIndices `[11,12,0,1]` plays
  `[11,0,1]`.

## 5. Character scheduling (sing/dance bookkeeping)

All OBSERVED in BaseCharacter/Bopper unless noted:

- Animation binding: character anim name (`idle`, `singLEFT`, `singLEFTmiss`,
  `idle-hold`, `danceLeft`…) with `prefix` (frame label), optional `frameIndices`,
  `frameRate ?? 24`, `looped ?? false`, `animType ?? "framelabel"`; prefixes match frame
  labels EXACTLY (e.g. "Idle", "Left Miss"); no case transformation.
- Dance trigger: `Bopper.onStepHit` → every `danceEvery*4` steps with `% 4 == 0` (per
  beat; danceEvery default 1.0) → `dance(false)`; `BaseCharacter.dance`:
  dead → return; currently singing (`name.startsWith("sing") && !endsWith("-end")`)
  → return; if the current animation is not dance*/idle* AND not finished → return;
  else Bopper.dance: if `hasAnimation('danceLeft')` the character ALTERNATES
  (`hasDanced ? danceRight : danceLeft`, first dance = danceLeft; the character data
  `startingAnimation` is parsed but inert in gameplay), otherwise plays `idle` with
  force=false — restarting at frame 0 if the previous idle finished, continuing otherwise.
- Sing trigger on note events (BaseCharacter L550-626): onNoteHit/miss for bf/dad plays
  `sing{DIR}` | `sing{DIR}miss` | `sing{DIR}-{suffix}` with restart=true and
  `holdTimer = 0`; depicts LEFT=0 DOWN=1 UP=2 RIGHT=3 (NoteDirection order) → anim names
  `singLEFT/singDOWN/singUP/singRIGHT`, miss forms are `singLEFTmiss` (no dash).
- `-hold` progression (BaseCharacter.onUpdate L389-471):
  1. Each update: if animation finished && !name endsWith("-hold") && `hasAnimation(name
     + "-hold")` → play(name+"-hold") — a continuing looped animation.
  2. While singing: `holdTimer += elapsed`; `singTimeSec = singTimeSteps *
     conductor.stepLengthMs/1000` — **singTime is in STEPS** (corrects the earlier
     "(seconds)" wording; for dad `8.0` = 8 steps, at BPM 100 8 × 150 ms = 1.2 s);
     miss animation timing doubles singTimeSec (L436); for bf the stop condition also
     requires the hold-note check (`!isHoldingNote()`); once
     `holdTimer > singTimeSec && shouldStopSinging`: if the current is
     X+`-hold` play X+`-hold`?? — no: strip `-hold` and then play X + `-end` if it
     exists, else `dance(true)` (restart forced).
  3. Not singing: holdTimer = 0.
- Flip: bf data `flipX: true`; Stage.addCharacter sets
  `character.flipX = !character.getDataFlipX()` for the PLAYER (bf) only (L425/431/436)
  — with bf.json `flipX: true` the FINAL flipX is FALSE: bf renders UNMIRRORED
  (falsified by the human playtest that saw bf facing the wrong way; 2026-11-07); dad/gf
  get `flipX = getDataFlipX()` (absent ⇒ false). **Derived** implementation note: the
  flixel path applies the flip as a whole-sprite matrix (scale(−1,1)+translate(bounds.w,0)
  inserted into the character matrix before sprite scale/rotation); the mirror applies
  only when the RESOLVED flip_x is true (bf: not, in this checkout).
- EOF note: `singTimeSteps * stepLengthMs/1000` uses the CONDUCTOR step length at the
  current ms — the same conductor model as M4 (1 ms grid semantics frozen there).

## 6. PlayState per-frame scene update order (what render_diff reproduces)

OBSERVED (PlayState.hx L1190-1360, §M5-CONTRACT §1.3/§1.4 freezes + ADR-0019 correction):
1. Conductor update + health clamp/death check at frame top.
2. Camera zoom/bop decay (L1217-1228).
3. Strumline/state updates via `super.update` first (the M4/ADR-0019 frame order), then
   `processInputQueue` → `processNotes` events (hits/misses/holds).
4. Character updates (holdTimer, `-hold` swap on finished animations) and bookkeeping.
5. Render (every state above frozen at the current song position).

**Engine decision (scene integration contract)**: the Rust renderer consumes the exact
same tick loop as the sim (1 ms grid) — `frame(t)` composes the state after tick t's
dispatch (camera/lerp edges computed with the same per-tick float ops; exact f32/f64
conversion policy per module documented by the implementer).

## 7. render_diff / perf_budget gate definitions (engine decisions — acceptance plan)

**render_diff** (engine decision, milestone 5):
- Harness: `cargo xtask render-diff --assets DIR` (explicit assets, fail-closed; no
  native game dependency). It replays the SAME frozen note-native streams
  (`tests/fixtures/note-native/*.stream.jsonl`, digests revalidated per run), steps the
  real `oxide_game::slice` sim (whose pipeline is already gated by
  `gameplay_sim_bopeebo`/`note_native`), and drives equal-tick scene updates.
- Sample frames: a FROZEN per-difficulty list of song positions (fixture manifest),
  chosen to cover: post-fade intro beats, per-judgement hits, mid-hold frames with
  clipped sustains/holds, hold drops, window misses, ghost, the 160/161 boundary presses,
  late song, and post-song freeze.
- Evidence: per-frame PNG under `target/render-diff/` (never committed); two-run sha256
  equality per frame and per difficulty; comparison against the frozen manifest digests
  (fail-closed on drift, NO fixture regeneration during verification); plus structural
  geometry assertions (receptor positions, note y at defined strum times, prop rects,
  character bounds) checked against constant values in the manifest — fail-closed.
- Honesty: golden frame digests are established ONCE from OUR renderer (an engine fixture
  per the ADR-0014/0019 pattern, after review). They capture renderer determinism and
  intentional-design regressions; they are NOT native pixel parity — a native-game
  rendered-output trace remains a separate pending obligation. The renderer runs the
  software-raster adapter PINNED (adapter recorded in the manifest; digests are
  adapter-specific — see the ADR-0020 addendum; GPU-driver/platform render output
  otherwise differs by adapter, so an adapter change is a fixture-replace event on the
  ADR path). Windows GPU render evidence stays separate and pending.
- If exact frame equality is infeasible for reasons unrelated to the renderer's own
  behavior (adapter quirks first observed in the establishment run), the fallback
  comparison method (e.g. per-frame absolute-diff/SSIM threshold) must be frozen by a
  BEFORE-USE ADR, never by editing the manifest during verification.

**perf_budget** (engine decision, milestone 5):
- Harness: `cargo xtask perf-budget --assets DIR`: same replayed scene (difficulty easy),
  HARDWARE adapter (ADR-0020 addendum decision 2; fails closed with typed
  `NoHardwareAdapter` when only a software adapter exists — llvmpipe NEVER
  substitutes), WARMUP frames (proposed 120, excluded) then H=300 measured frames
  rendered sequentially (both counts printed).
- Budget (native 60 Hz target): mean frame wall time ≤ 16.667 ms AND max frame ≤ 33.333
  ms over the measured window, PLATFORM-SPECIFIC hardware measurement (NVIDIA
  GTX 1050 Ti on this machine; NOT llvmpipe; NOT a real-game/release-binary
  latency claim). Full-slice per-frame budget
  changes require the ordinary ADR/review/human path; never silently loosen.
- Honesty: software-raster timings are NOT real-GPU/hardware evidence; Windows GPU driver
  and hardware latency remain separate platform-pending obligations of M5/M6.

## 8. Dependencies/provenance summary and honesty

- Analyst reports (this session, 2026-11-07): character/atlas → §4-5;
  stage/camera/strumline → §1-3. The headless render probe entry: §7 pinning
  (`WGPU_ADAPTER_NAME=llvmpipe` software Vulkan rendered byte-identical PNGs across two
  runs (on the llvmpipe adapter) on this machine; the probe's default wgpu backend
  selection segfaulted, but pinning `WGPU_BACKEND=vulkan` on the NVIDIA adapter worked
  (see the ADR-0020 addendum: hardware adapter works at 3.9–14.4 ms/frame in release);
  the software adapter is still pinned for render_diff determinism, hardware is used by
  perf_budget only).
- Matrix arithmetic: the atlas resolver keeps f64 (Haxe `Float` is double); Bevy sprite
  positions are f32 — the pixel fixtures pin our own chosen composition, NOT a claim of
  bit-exact openfl equivalence; label this divergence explicitly in the gate report.
- No original Haxe/source snippets are pasted into Rust code; this document carries the
  numeric behavior. Implementer workers operate specification-only: request
  clarifications through the orchestrator instead of reading Haxe directly (per
  `docs/WORKFLOW.md` §2 for the visual slice; accidental exposure must be recorded).
- Out-of-scope still pending after phase 2b: rendered-output native-game trace, judge
  popups/health bar/HUD, camera cutout machinery on non-desktop sizes, Windows GPU
  render, real input latency, audio waveform verification.
