import funkin.Conductor;
import funkin.data.song.SongData.SongTimeChange;
import flixel.FlxG;
import flixel.sound.FlxSound;

/**
 * Native probe executing the REAL funkin.Conductor (installed game source,
 * unrelated fields sliced off before typing by ConductorSlice) against the
 * frozen replay recipe (docs/M4-CONTRACT.md §1; tools/oracle/BEHAVIOR.md):
 *
 *   1. map (real mapTimeChanges; real SongDataUtils sort via installed FlxSort),
 *   2. stopped pre-roll at (beatLengthMs * -5) + 0, update(pre, false),
 *   3. countdown: while songPosition < startTimestamp + combinedOffset:
 *      update(previous + 1 ms, false),
 *   4. playback: music.playing = true; update(musicTime + combinedOffset, false)
 *      for every integer music time 0..floor(lengthMs) inclusive.
 *
 * Every `sim` is `0` for pre-roll then +1 per update; every update is replayed
 * (silent ones emit no rows but consume a sim). Unlike the Neko model
 * (OracleConductor), NOTHING here is transcribed from Conductor.hx — the
 * compiled code is the game's own class; only its unrelated fields are
 * removed. Playback calls use the model's argument shape
 * (update(songPos_input, false), songPos_input = music.time + combinedOffset)
 * so the `input` row field is comparable 1:1 with the frozen fixtures.
 *
 * Evidence limits (also printed to the output):
 *  - PlayState is stubbed: get_bpm()'s startingSong scan never runs (the
 *    corpus scans select timeChanges[0], identical to the un-stubbed update()
 *    semantics; startTimestamp is 0). Event/order equivalence, not code identity.
 *  - `Float.clamp` in the playback branch is the real installed FloatTools.
 *  - No hardware/audio: the simulated music object supplies time/length.
 *    That is the same model assumption as the frozen fixtures, NOT real-game
 *    audio engine integration.
 *
 * Output: one JSONL per `--input` manifest entry, written to the directory
 * given by --out: `<id>.jsonl` for the 61 charts and `_smoke.jsonl`. Row
 * format/fields are identical to tests/fixtures/conductor/*.jsonl.
 */
@:privateAccess
class NativeConductorProbe
{
  static var firedRowKinds:Array<String> = [];

  static function main():Void
  {
    var inputPath = Sys.getEnv("PROBE_INPUT_JSON");
    if (inputPath == null || inputPath.length == 0)
    {
      Sys.stderr().writeString("native probe: PROBE_INPUT_JSON is not set\n");
      Sys.exit(2);
    }
    var outDir = Sys.getEnv("PROBE_OUT_DIR");
    if (outDir == null || outDir.length == 0)
    {
      Sys.stderr().writeString("native probe: PROBE_OUT_DIR is not set\n");
      Sys.exit(2);
    }
    sys.FileSystem.createDirectory(outDir);

    var manifest:Manifest = haxe.Json.parse(sys.io.File.getContent(inputPath));

    var rows:Map<String, String> = new Map();
    simulateChart(manifest.smoke, "_smoke", rows);
    for (chart in manifest.charts)
    {
      simulateChart(chart, chart.id, rows);
    }

    // Deterministic file write order: ids sorted lexicographically.
    var ids:Array<String> = []; for (k in rows.keys()) ids.push(k);
    ids.sort(function(a:String, b:String) return Reflect.compare(a, b));
    for (id in ids)
    {
      sys.io.File.saveContent('${outDir}/${id}.jsonl', rows[id]);
    }
    Sys.stdout().writeString("native-conductor-probe: timelines " + ids.length + "\n");
  }

  static function simulateChart(entry:ChartEntry, outId:String, rows:Map<String, String>):Void
  {
    // Fresh conductor per chart, then read the singleton on first use so the
    // retained static plumbing sets itself up. Rows are recorded from the
    // game's own onStepHit/onBeatHit/onMeasureHit signals: they fire exactly
    // when the corresponding int changed, in the update()'s dispatch order.
    // The simulated music object is a static flixel state: clear it before
    // every chart so the pre-roll runs through the retained update()'s
    // stopped (non-clamp) branch, exactly like the frozen recipe.
    FlxG.sound.music = null;
    Conductor.reset();
    var conductor:Conductor = Conductor.instance;
    conductor.onStepHit.add(function() firedRowKinds.push("step"));
    conductor.onBeatHit.add(function() firedRowKinds.push("beat"));
    conductor.onMeasureHit.add(function() firedRowKinds.push("measure"));
    rows.set(outId, "");

    conductor.instrumentalOffset = entry.iOff;
    // Desktop build: format offset stays 0 (observed Constants EXT_SOUND
    // behavior; the manifest records fOff and it equals 0 for every entry).
    conductor.formatOffset = entry.fOff;
    // Calibration inputs (observed defaults are 0 for the whole corpus and
    // they are recorded in the manifest like iOff/fOff).
    funkin.Preferences.globalOffset = entry.gOff;
    // audioVisualOffset is an input read by the retained real getter
    // (`Save?.instance?.options?.audioVisualOffset ?? 0`). The corpus records
    // avOff = 0 for every entry, and the probe leaves the save null so the
    // fallback path applies; a nonzero recorded avOff would fail closed.
    if (entry.avOff != 0)
    {
      Sys.stderr().writeString("probe: recorded avOff != 0 (input truth requires a propagated save stub); refusing\n");
      Sys.exit(2);
    }
    funkin.save.Save.instance = null;

    // The simulated playback object: same role the model gave musicPlaying/
    // musicLength. Also verified against the frozen manifest lengths.
    var music:FlxSound = new FlxSound();
    music.length = entry.lengthMs;

    var changes:Array<SongTimeChange> = [];
    for (raw in entry.timeChanges)
    {
      // beatTime (`b`) carries the raw recorded value exactly like the model.
      changes.push(new SongTimeChange(raw.t, raw.bpm, raw.n, raw.d, raw.b));
    }
    conductor.mapTimeChanges(changes);

    // (see the signal listeners added above the pre-roll block)
    var lineBuffer:String = "";
    var stepCount:Int = 0;
    var beatCount:Int = 0;
    var measureCount:Int = 0;
    var sim:Int = 0;

    // 1. Pre-roll (music still null → update's stopped branch).
    var pre:Float = (conductor.beatLengthMs * -5.0) + 0.0;
    firedRowKinds = [];
    conductor.update(pre, false);
    var fired:Array<String> = firedRowKinds.copy();
    var update = { sim: sim, input: pre, fired: fired };
    var counts = appendRows(update, conductor, lineBuffer);
    lineBuffer = counts.lines;
    stepCount += countKind(fired, "step");
    beatCount += countKind(fired, "beat");
    measureCount += countKind(fired, "measure");

    // 2. Countdown, stopped.
    while (conductor.songPosition < 0.0 + conductor.combinedOffset)
    {
      sim += 1;
      var nextInput:Float = conductor.songPosition + 1.0;
      firedRowKinds = [];
      conductor.update(nextInput, false);
      fired = firedRowKinds.copy();
      update = { sim: sim, input: nextInput, fired: fired };
      counts = appendRows(update, conductor, lineBuffer);
      lineBuffer = counts.lines;
      stepCount += countKind(fired, "step");
      beatCount += countKind(fired, "beat");
      measureCount += countKind(fired, "measure");
    }

    // 3. Playback ... (loop below)
    music.playing = true;
    FlxG.sound.music = music;
    var musicTime:Float = 0.0;
    while (musicTime <= entry.lengthMs)
    {
      sim += 1;
      var input:Float = musicTime + conductor.combinedOffset;
      firedRowKinds = [];
      conductor.update(input, false);
      fired = firedRowKinds.copy();
      update = { sim: sim, input: input, fired: fired };
      counts = appendRows(update, conductor, lineBuffer);
      lineBuffer = counts.lines;
      stepCount += countKind(fired, "step");
      beatCount += countKind(fired, "beat");
      measureCount += countKind(fired, "measure");
      musicTime += 1.0;
    }

    rows.set(outId, lineBuffer);
    var countsLine = 'counts ${outId} rows=${stepCount + beatCount + measureCount} steps=${stepCount} beats=${beatCount} measures=${measureCount}';
    Sys.stdout().writeString(countsLine + "\n");
  }

  static function countKind(list:Array<String>, kind:String):Int
  {
    var n:Int = 0;
    for (k in list) if (k == kind) n++;
    return n;
  }

  // Reference the retained private fields; Haxe needs per-function opt-in.
  @:privateAccess
  static function appendRows(update:{sim:Int, input:Float, fired:Array<String>}, conductor:Conductor, buffer:String):{lines:String}
  {
    // Identify the current time change index the way the model did: by
    // reference identity in the mapped array.
    var tcIndex:Int = -1;
    @:privateAccess
    for (i in 0...conductor.timeChanges.length)
    {
      if (conductor.timeChanges[i] == conductor.currentTimeChange)
      {
        tcIndex = i;
        break;
      }
    }

    for (kind in update.fired)
    {
      var i:Int = switch (kind)
      {
        case "step": conductor.currentStep;
        case "beat": conductor.currentBeat;
        case "measure": conductor.currentMeasure;
        default: throw "unexpected fired kind: " + kind;
      };
      buffer += '{"t":"' + kind
        + '","i":' + Std.string(i)
        + ',"sim":' + Std.string(update.sim)
        + ',"input":' + Std.string(update.input)
        + ',"songPosition":' + Std.string(conductor.songPosition)
        + ',"stepTime":' + Std.string(conductor.currentStepTime)
        + ',"beatTime":' + Std.string(conductor.currentBeatTime)
        + ',"measureTime":' + Std.string(conductor.currentMeasureTime)
        + ',"step":' + Std.string(conductor.currentStep)
        + ',"beat":' + Std.string(conductor.currentBeat)
        + ',"measure":' + Std.string(conductor.currentMeasure)
        + ',"beatLen":' + Std.string(conductor.beatLengthMs)
        + ',"stepLen":' + Std.string(conductor.stepLengthMs)
        + ',"measureLen":' + Std.string(conductor.measureLengthMs)
        + ',"tc":' + Std.string(tcIndex)
        + ',"tsNum":' + Std.string(conductor.timeSignatureNumerator)
        + ',"tsDen":' + Std.string(conductor.timeSignatureDenominator)
        + ',"bpm":' + Std.string(conductor.bpm)
        + ',"iOff":' + Std.string(conductor.instrumentalOffset)
        + ',"fOff":' + Std.string(conductor.formatOffset)
        + ',"avOff":' + Std.string(0)
        + ',"gOff":' + Std.string(conductor.globalOffset)
        + ',"combined":' + Std.string(conductor.combinedOffset)
        + ',"clampActive":' + (FlxG.sound.music != null && FlxG.sound.music.playing ? "true" : "false")
        + '}\n';
    }
    return {lines: buffer};
  }
}

typedef ProbeTimeChange = {
  t:Float,
  b:Float,
  bpm:Float,
  n:Int,
  d:Int
};

typedef ChartEntry = {
  id:String,
  lengthMs:Float,
  iOff:Float,
  fOff:Float,
  avOff:Int,
  gOff:Int,
  timeChanges:Array<ProbeTimeChange>
};

typedef Manifest = {
  schema:Int,
  charts:Array<ChartEntry>,
  smoke:ChartEntry
};
