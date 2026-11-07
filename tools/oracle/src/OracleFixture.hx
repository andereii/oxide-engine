import OracleAssets.OracleMetadata;
import OracleConductor.ConductorDispatch;
import sys.io.File;

/**
 * Turns an `OracleConductor` into the frozen JSONL timeline described by
 * docs/M4-CONTRACT.md §1.
 *
 * Phases reproduced from PlayState:
 *  - create(): pre = beatLengthMs * -5 + startTimestamp, then update(pre),
 *    with music stopped (source/funkin/play/PlayState.hx:L878-883).
 *  - countdown: update(songPosition + 1.0, false) until
 *    songPosition >= startTimestamp + combinedOffset (L1147-1158; elapsed is
 *    fixed to 1 ms by the fixture contract, startTimestamp = 0).
 *  - playback: music.playing = true, music.time advances 0 -> length in 1 ms
 *    steps, input = music.time + combinedOffset, update(input, false)
 *    (L1172-1180 with perfect lerp). The L440-444 clamp is active here.
 *
 * `sim` is the runner's simulation clock in milliseconds: 0 for the initial
 * pre-roll update, then +1 per countdown/playback millisecond. It is not the
 * audio position (`input` and `songPosition` are), and it is part of the
 * frozen snapshot.
 */
class OracleFixture {
  public static function simulate(metadata:OracleMetadata, lengthMs:Float):OracleSimulation {
    var conductor = new OracleConductor();
    conductor.instrumentalOffset = metadata.instrumentalOffset;
    // Desktop build: EXT_SOUND = 'ogg', so formatOffset stays 0
    // (Constants.hx:L618; PlayState.hx:L1159-1163 is web-only).
    conductor.formatOffset = 0.0;
    // No save file exists for the oracle run: Preferences.globalOffset = 0
    // (Preferences.hx:L317-322) and audioVisualOffset = 0 (Conductor.hx:L289-292).
    conductor.globalOffset = 0;
    conductor.audioVisualOffset = 0;
    conductor.startTimestamp = 0.0;
    conductor.musicLength = lengthMs;

    conductor.mapTimeChanges(metadata.changes);

    var lines:Array<String> = [];
    var stepCount:Int = 0;
    var beatCount:Int = 0;
    var measureCount:Int = 0;
    var sim:Float = 0.0;

    // = PlayState.hx:L878: pre = (beatLengthMs * -5) + startTimestamp.
    var pre:Float = (conductor.beatLengthMs * -5) + conductor.startTimestamp;

    // = PlayState.hx:L883: Conductor.instance.update(pre). The M4 fixture
    // contract freezes applyOffsets = false for the pre-roll phase; with the corpus
    // combinedOffset = 0 the reference's default-true call is numerically
    // identical. `input` records the argument before the offset addition.
    var fired = conductor.update(pre, false);
    appendRows(lines, conductor, fired, sim, pre);
    var firedCounts = countDispatches(fired);
    stepCount += firedCounts.steps;
    beatCount += firedCounts.beats;
    measureCount += firedCounts.measures;

    // = PlayState.hx:L944 and L1147-1158: countdown loop, music stopped.
    conductor.startingSong = true;
    while (conductor.songPosition < conductor.startTimestamp + conductor.combinedOffset) {
      var nextInput:Float = conductor.songPosition + 1.0;
      sim += 1.0;
      fired = conductor.update(nextInput, false);
      appendRows(lines, conductor, fired, sim, nextInput);
      firedCounts = countDispatches(fired);
      stepCount += firedCounts.steps;
      beatCount += firedCounts.beats;
      measureCount += firedCounts.measures;
    }

    // = PlayState.hx:L2634-2660: startSong() sets startingSong = false,
    // music.playing = true and music.time = startTimestamp.
    conductor.startingSong = false;
    conductor.musicPlaying = true;

    // = PlayState.hx:L1172-1180: perfect audio => input = music.time + combinedOffset.
    var musicTime:Float = 0.0;
    while (musicTime <= lengthMs) {
      var input:Float = musicTime + conductor.combinedOffset;
      sim += 1.0;
      fired = conductor.update(input, false);
      appendRows(lines, conductor, fired, sim, input);
      firedCounts = countDispatches(fired);
      stepCount += firedCounts.steps;
      beatCount += firedCounts.beats;
      measureCount += firedCounts.measures;
      musicTime += 1.0;
    }

    return {
      lines: lines,
      steps: stepCount,
      beats: beatCount,
      measures: measureCount
    };
  }

  /**
   * Hand-crafted smoke chart: 7/8 at 120 BPM with two BPM changes
   * (150 BPM at 1000 ms, 90 BPM at 2500 ms) and a 4000 ms instrumental.
   * It exercises negative pre-roll indexes, the 7/8 geometry and the
   * time-change segment accumulation without depending on the asset tree.
   */
  public static function smokeMetadata():OracleMetadata {
    return {
      changes: [
        new OracleTimeChange(0.0, 120.0, 7, 8),
        new OracleTimeChange(1000.0, 150.0, 7, 8),
        new OracleTimeChange(2500.0, 90.0, 7, 8)
      ],
      instrumentalOffset: 0.0,
      timeFormat: "ms"
    };
  }

  public static function simulateSmoke():OracleSimulation {
    return simulate(smokeMetadata(), 4000.0);
  }

  public static function writeFile(path:String, lines:Array<String>):Void {
    var buffer = new StringBuf();
    for (line in lines) {
      buffer.add(line);
      buffer.add("\n");
    }
    File.saveContent(path, buffer.toString());
  }

  static function appendRows(lines:Array<String>, conductor:OracleConductor, fired:Array<ConductorDispatch>, sim:Float, input:Float):Void {
    for (dispatch in fired) {
      lines.push(formatRow(conductor, dispatch, sim, input));
    }
  }

  static function countDispatches(fired:Array<ConductorDispatch>):DispatchCounts {
    var counts:DispatchCounts = {steps: 0, beats: 0, measures: 0};
    for (dispatch in fired) {
      switch (dispatch.kind) {
        case "step": counts.steps++;
        case "beat": counts.beats++;
        case "measure": counts.measures++;
      }
    }

    return counts;
  }

  static function formatRow(conductor:OracleConductor, dispatch:ConductorDispatch, sim:Float, input:Float):String {
    // bpm runs the startingSong scan first so tc and bpm are consistent.
    var bpm = conductor.bpm;
    var tc = conductor.currentChangeIndex();
    var tsNum = conductor.timeSignatureNumerator;
    var tsDen = conductor.timeSignatureDenominator;

    return '{"t":"' + dispatch.kind
      + '","i":' + Std.string(dispatch.value)
      + ',"sim":' + Std.string(sim)
      + ',"input":' + Std.string(input)
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
      + ',"tc":' + Std.string(tc)
      + ',"tsNum":' + Std.string(tsNum)
      + ',"tsDen":' + Std.string(tsDen)
      + ',"bpm":' + Std.string(bpm)
      + ',"iOff":' + Std.string(conductor.instrumentalOffset)
      + ',"fOff":' + Std.string(conductor.formatOffset)
      + ',"avOff":' + Std.string(conductor.audioVisualOffset)
      + ',"gOff":' + Std.string(conductor.globalOffset)
      + ',"combined":' + Std.string(conductor.combinedOffset)
      + ',"clampActive":' + (conductor.clampActive ? "true" : "false")
      + '}';
  }
}

typedef OracleSimulation = {
  var lines:Array<String>;
  var steps:Int;
  var beats:Int;
  var measures:Int;
};

typedef DispatchCounts = {
  var steps:Int;
  var beats:Int;
  var measures:Int;
};
