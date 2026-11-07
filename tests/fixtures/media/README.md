# Media mini fixture: synthetic conversion coverage

`mini/songs/synthetic/{Inst,Voices-bf}.ogg` are **synthetic seeded white noise**,
not recordings or extracts from copyrighted music. The existing four low-bitrate
`.ogg` fixtures remain unchanged and exercise byte-copy policy. The new pair
exercises actual Opus conversion, positive duration-check coverage, and relative
stem alignment through `pack_and_verify` / `xtask media-check`.

Generated with ffmpeg **8.1.3**, built with gcc 16 (GCC), with libvorbis/libopus support;
ffprobe **8.1.3** was used for the measurements below. Commands run from the
workspace root (fixed seeds, environment, metadata and bitexact Ogg container):

```sh
LC_ALL=C LANG=C TZ=UTC ffmpeg -y -v error -nostdin -f lavfi -i 'anoisesrc=sample_rate=48000:duration=4:color=white:amplitude=0.3:seed=13013' -af 'afade=t=in:st=0:d=0.02,afade=t=out:st=3.2:d=0.1,adelay=240:all=1,atrim=duration=4' -map_metadata -1 -c:a libvorbis -q:a 10 -fflags +bitexact tests/fixtures/media/mini/songs/synthetic/Inst.ogg
LC_ALL=C LANG=C TZ=UTC ffmpeg -y -v error -nostdin -f lavfi -i 'anoisesrc=sample_rate=48000:duration=4:color=white:amplitude=0.3:seed=14014' -af 'afade=t=in:st=0:d=0.02,afade=t=out:st=3.2:d=0.1,adelay=610:all=1,atrim=duration=4' -map_metadata -1 -c:a libvorbis -q:a 10 -fflags +bitexact tests/fixtures/media/mini/songs/synthetic/Voices-bf.ogg
```

Both are mono 48 kHz, four seconds of decoded content, quality-10 Vorbis in Ogg,
so the test exercises Vorbis decoding as well as Opus encoding. ffprobe reports
duration 4.000000 s and sizes 116,231 / 116,313 bytes (effective 232 kbps including
overhead). Distinct 240 ms / 610 ms leading delays make the
source stems deliberately non-aligned; conversion must preserve their relative
alignment, not force the stems to have the same onset.

An isolated recipe check using the pipeline's unmodified `libopus -b:a 112k`
(Inst) / `64k` (Voices), `-vbr on -fflags +bitexact` produced 55,019 / 28,485 bytes
respectively, both with duration 4.006500 s (a 6.5 ms delta within the unchanged
50 ms tolerance). These measurements are **not** a
substitute for the Rust end-to-end tests. Cross-version encoded bytes may differ.

## Acceptance commands (parent runner; not executed during this change)

```sh
cargo test --locked -p oxide-media
cargo test --locked -p xtask
cargo test --locked -p oxide-media --lib cross_correlation_recovers_synthetic_delay -- --include-ignored
cargo test --locked -p oxide-media --test media_mini -- --include-ignored
cargo test --locked -p xtask --test media_mini -- --include-ignored
cargo xtask parse-all --corpus --dir /home/alex/Descargas/fnf/Funkin/assets
cargo xtask parse-atlas --corpus --dir /home/alex/Descargas/fnf/Funkin/assets
cargo xtask media-check --corpus --in /home/alex/Descargas/fnf/Funkin/assets --out assets-dist
```

External-tool tests are explicitly ignored in ordinary test runs. Acceptance
includes them; missing ffmpeg/ffprobe is then a failure, never a successful early
return. No numeric verification tolerances were changed. The `--corpus` flag
pins this reference inventory only; arbitrary mods do not inherit its counts.
