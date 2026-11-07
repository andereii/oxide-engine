
/**
 * Standalone, instrumented transcription of `funkin.Conductor` (Haxe oracle).
 *
 * This file deliberately imports nothing from the game or from Flixel: the
 * M4 contract freezes the conductor maths as f64 (`Float` in Haxe) and the
 * fixture snapshot must be reproducible without compiling the game. Every
 * method carries a `// = funkin/...:Lx-y` citation; the same ranges are listed
 * in `Verbatim.hx` and hashed by `snapshot_validate.sh`.
 *
 * The full timeline is a model, not an independent execution of the game.
 * Only utility rounding has native runtime/real-class probe evidence here.
 * See tools/oracle/BEHAVIOR.md and ADR-0014 for the evidence boundary.
 */
class OracleConductor {
  // = funkin/util/Constants.hx:L368 (SECS_PER_MIN = 60)
  public static final SECS_PER_MIN:Float = 60.0;
  // = funkin/util/Constants.hx:L373 (MS_PER_SEC = 1000)
  public static final MS_PER_SEC:Float = 1000.0;
  // = funkin/util/Constants.hx:L424 (STEPS_PER_BEAT = 4)
  public static final STEPS_PER_BEAT:Int = 4;
  // = funkin/util/Constants.hx:L261 (DEFAULT_BPM = 100)
  public static final DEFAULT_BPM:Float = 100.0;
  // = funkin/util/Constants.hx:L326,L331 (default time signature 4/4)
  public static final DEFAULT_TIME_SIGNATURE_NUM:Int = 4;
  public static final DEFAULT_TIME_SIGNATURE_DEN:Int = 4;

  // = funkin/Conductor.hx:L96-101
  public var timeChanges:Array<OracleTimeChange> = [];
  public var currentTimeChange:Null<OracleTimeChange>;

  // = funkin/Conductor.hx:L107
  public var songPosition:Float = 0.0;

  // = funkin/Conductor.hx:L218-246
  public var currentMeasure:Int = 0;
  public var currentBeat:Int = 0;
  public var currentStep:Int = 0;
  public var currentMeasureTime:Float = 0.0;
  public var currentBeatTime:Float = 0.0;
  public var currentStepTime:Float = 0.0;

  // = funkin/Conductor.hx:L251,L268-299
  public var instrumentalOffset:Float = 0.0;
  public var formatOffset:Float = 0.0;
  public var globalOffset:Int = 0;
  public var audioVisualOffset:Int = 0;

  // = funkin/Conductor.hx:L123-140
  public var bpmOverride:Null<Float> = null;
  /** PlayState.startingSong: true while the countdown mutates get_bpm(). */
  public var startingSong:Bool = false;
  /** PlayState.startTimestamp; the fixture contract fixes this to 0. */
  public var startTimestamp:Float = 0.0;

  /**
   * The simulated FlxG.sound.music flags. `musicPlaying` is the L440 branch
   * (`music.playing`); `musicLength` is the decoded instrumental duration in
   * milliseconds and is read by the clamp at L442.
   */
  public var musicPlaying:Bool = false;
  public var musicLength:Float = 0.0;
  /** Mirrors `FlxG.sound.music != null && FlxG.sound.music.playing`. */
  public var clampActive:Bool = false;

  public function new() {}

  // = funkin/Conductor.hx:L166-171
  public var measureLengthMs(get, never):Float;

  function get_measureLengthMs():Float {
    return beatLengthMs * timeSignatureNumerator;
  }

  // = funkin/Conductor.hx:L176-182
  public var beatLengthMs(get, never):Float;

  function get_beatLengthMs():Float {
    return ((SECS_PER_MIN / bpm) * MS_PER_SEC) * (4 / timeSignatureDenominator);
  }

  // = funkin/Conductor.hx:L187-192
  public var stepLengthMs(get, never):Float;

  function get_stepLengthMs():Float {
    return beatLengthMs / STEPS_PER_BEAT;
  }

  // = funkin/Conductor.hx:L123-140
  public var bpm(get, never):Float;

  function get_bpm():Float {
    if (bpmOverride != null) return bpmOverride;

    if (currentTimeChange == null) return DEFAULT_BPM;

    // The reference scans the time changes while startingSong, using the
    // PlayState start timestamp. The fixture contract fixes startTimestamp = 0,
    // which selects timeChanges[0] and is already the value update() selected
    // for songPosition <= 0. Kept verbatim so the audit sees the same branch.
    if (startingSong) {
      for (i in 0...timeChanges.length) {
        if (startTimestamp >= timeChanges[i].timeStamp) currentTimeChange = timeChanges[i];

        if (startTimestamp < timeChanges[i].timeStamp) break;
      }
    }

    return currentTimeChange.bpm;
  }

  // = funkin/Conductor.hx:L197-216
  public var timeSignatureNumerator(get, never):Int;

  function get_timeSignatureNumerator():Int {
    if (currentTimeChange == null) return DEFAULT_TIME_SIGNATURE_NUM;

    return currentTimeChange.timeSignatureNum;
  }

  public var timeSignatureDenominator(get, never):Int;

  function get_timeSignatureDenominator():Int {
    if (currentTimeChange == null) return DEFAULT_TIME_SIGNATURE_DEN;

    return currentTimeChange.timeSignatureDen;
  }

  // = funkin/Conductor.hx:L301-319
  public var stepsPerMeasure(get, never):Int;

  function get_stepsPerMeasure():Int {
    return Std.int(timeSignatureNumerator * STEPS_PER_BEAT);
  }

  // = funkin/Conductor.hx:L294-299
  public var combinedOffset(get, never):Float;

  function get_combinedOffset():Float {
    return instrumentalOffset + formatOffset + globalOffset;
  }

  /**
   * `FlxMath.roundDecimal(value, precision)`, transcribed exactly from the
   * installed HaxeFlixel:
   *
   *   .haxelib/flixel/git/flixel/math/FlxMath.hx:L68-76
   *   var mult:Float = 1;
   *   for (i in 0...Precision) mult *= 10;
   *   return Math.fround(Value * mult) / mult;
   *
   * hxcpp include/hxMath.h:28 defines fround as std::floor(x + 0.5),
   * NOT a binary32 cast. Native real-FlxMath evidence: RealRoundProbe.hx.
   * Use ffloor to retain a Float-valued integer on Neko too; its fround
   * fallback is Int-valued and can overflow for scaled late-song steps.
   */
  public static function roundDecimal(value:Float, precision:Int):Float {
    var mult:Float = 1.0;
    for (i in 0...precision) mult *= 10.0;

    return Math.ffloor(value * mult + 0.5) / mult;
  }

  // = funkin/Conductor.hx:L549-600
  public function mapTimeChanges(songTimeChanges:Array<OracleTimeChange>):Void {
    timeChanges = [];

    // = funkin/data/song/SongDataUtils.hx:L267-275 (FlxSort.ASCENDING on timeStamp)
    var sorted:Array<OracleTimeChange> = songTimeChanges.copy();
    insertionSortByTimeStamp(sorted);

    for (songTimeChange in sorted) {
      // = funkin/Conductor.hx:L561
      if (songTimeChange.timeStamp < 0.0) songTimeChange.timeStamp = 0.0;

      if (songTimeChange.timeStamp <= 0.0) {
        // = funkin/Conductor.hx:L563-566
        songTimeChange.beatTime = 0.0;
      } else {
        // = funkin/Conductor.hx:L567-580
        songTimeChange.beatTime = 0.0;

        if (songTimeChange.timeStamp > 0.0 && timeChanges.length > 0) {
          var prevTimeChange:OracleTimeChange = timeChanges[timeChanges.length - 1];
          songTimeChange.beatTime = roundDecimal(prevTimeChange.beatTime
            + ((songTimeChange.timeStamp - prevTimeChange.timeStamp) * prevTimeChange.bpm / SECS_PER_MIN / MS_PER_SEC * (prevTimeChange.timeSignatureDen / 4)),
            4);
        }
      }

      timeChanges.push(songTimeChange);
    }

    // = funkin/Conductor.hx:L598-599
    this.update(this.songPosition, false);
  }

  /**
   * Stable ascending sort by timeStamp, equivalent to
   * `SongDataUtils.sortTimeChanges` for the (unique-timestamp) V-Slice data.
   */
  static function insertionSortByTimeStamp(values:Array<OracleTimeChange>):Void {
    var i:Int = 1;
    while (i < values.length) {
      var j:Int = i;
      while (j > 0 && values[j - 1].timeStamp > values[j].timeStamp) {
        var tmp:OracleTimeChange = values[j];
        values[j] = values[j - 1];
        values[j - 1] = tmp;
        j--;
      }
      i++;
    }
  }

  // = funkin/Conductor.hx:L422-517
  public function update(?songPos:Float, applyOffsets:Bool = true, forceDispatch:Bool = false):Array<ConductorDispatch> {
    // = funkin/Conductor.hx:L424-425: the simulated music object always exists.
    var currentLength:Float = musicLength;

    if (songPos == null) songPos = 0.0;

    // = funkin/Conductor.hx:L432-433
    songPos += applyOffsets ? combinedOffset : 0.0;

    // = funkin/Conductor.hx:L435-437
    var oldMeasure:Int = this.currentMeasure;
    var oldBeat:Int = this.currentBeat;
    var oldStep:Int = this.currentStep;

    // = funkin/Conductor.hx:L439-448
    clampActive = musicPlaying;
    if (musicPlaying) {
      var minPosition:Float = Math.min(combinedOffset, 0.0);
      // Floats.clamp(value, min, max) checks the lower bound first.
      this.songPosition = songPos < minPosition ? minPosition : (songPos > currentLength ? currentLength : songPos);
    } else {
      this.songPosition = songPos;
    }

    // = funkin/Conductor.hx:L452-461
    currentTimeChange = timeChanges.length > 0 ? timeChanges[0] : null;
    if (this.songPosition > 0.0) {
      for (i in 0...timeChanges.length) {
        if (this.songPosition >= timeChanges[i].timeStamp) currentTimeChange = timeChanges[i];

        if (this.songPosition < timeChanges[i].timeStamp) break;
      }
    }

    // = funkin/Conductor.hx:L463-487. The source has a warning branch for
    // empty timeChanges while a music object exists; docs/M4-CONTRACT.md §0.5
    // freezes the constant-BPM else branch for that edge case (ADR-0006), so
    // the oracle always evaluates timing here and reports tc = -1 with the
    // default 100 BPM / 4-4 geometry.
    if (currentTimeChange != null && this.songPosition > 0.0) {
      // = funkin/Conductor.hx:L470-476
      this.currentStepTime = roundDecimal((currentTimeChange.beatTime * STEPS_PER_BEAT)
        + (this.songPosition - currentTimeChange.timeStamp) / stepLengthMs, 6);
      this.currentBeatTime = currentStepTime / STEPS_PER_BEAT;
      this.currentMeasureTime = getTimeInMeasures(this.songPosition);
      this.currentStep = Math.floor(currentStepTime);
      this.currentBeat = Math.floor(currentBeatTime);
      this.currentMeasure = Math.floor(currentMeasureTime);
    } else {
      // = funkin/Conductor.hx:L478-487
      this.currentStepTime = roundDecimal((this.songPosition / stepLengthMs), 4);
      this.currentBeatTime = currentStepTime / STEPS_PER_BEAT;
      this.currentMeasureTime = currentStepTime / stepsPerMeasure;
      this.currentStep = Math.floor(currentStepTime);
      this.currentBeat = Math.floor(currentBeatTime);
      this.currentMeasure = Math.floor(currentMeasureTime);
    }

    // = funkin/Conductor.hx:L489-503: dispatch order step -> beat -> measure.
    var fired:Array<ConductorDispatch> = [];
    if (forceDispatch || currentStep != oldStep) fired.push({kind: "step", value: currentStep});
    if (forceDispatch || currentBeat != oldBeat) fired.push({kind: "beat", value: currentBeat});
    if (forceDispatch || currentMeasure != oldMeasure) fired.push({kind: "measure", value: currentMeasure});

    return fired;
  }

  // = funkin/Conductor.hx:L607-647
  public function getTimeInMeasures(ms:Float):Float {
    if (timeChanges.length == 0) {
      return ms / stepLengthMs / stepsPerMeasure;
    } else {
      var resultMeasureTime:Float = 0.0;
      ms = ms < 0.0 ? 0.0 : ms;

      var lastTimeChange:OracleTimeChange = timeChanges[0];
      var i:Int = -1;
      for (timeChange in timeChanges) {
        if (ms >= timeChange.timeStamp) {
          if (ms < timeChange.timeStamp || i == timeChanges.length - 1) {
            lastTimeChange = timeChange;
            break;
          }

          var currentStepLengthMs:Float = (((SECS_PER_MIN / lastTimeChange.bpm) * MS_PER_SEC) * (4 / lastTimeChange.timeSignatureDen)) / STEPS_PER_BEAT;
          var currentStepsPerMeasure:Int = lastTimeChange.timeSignatureNum * STEPS_PER_BEAT;
          resultMeasureTime += (timeChange.timeStamp - lastTimeChange.timeStamp) / currentStepLengthMs / currentStepsPerMeasure;
          lastTimeChange = timeChange;
        }
        i++;
      }

      var remainingStepLengthMs:Float = (((SECS_PER_MIN / lastTimeChange.bpm) * MS_PER_SEC) * (4 / lastTimeChange.timeSignatureDen)) / STEPS_PER_BEAT;
      var remainingStepsPerMeasure:Int = lastTimeChange.timeSignatureNum * STEPS_PER_BEAT;
      var remainingFractionalMeasure:Float = (ms - lastTimeChange.timeStamp) / remainingStepLengthMs / remainingStepsPerMeasure;
      resultMeasureTime += remainingFractionalMeasure;

      return resultMeasureTime;
    }
  }

  public function currentChangeIndex():Int {
    if (currentTimeChange == null) return -1;

    for (i in 0...timeChanges.length) {
      if (timeChanges[i] == currentTimeChange) return i;
    }

    return -1;
  }
}

typedef ConductorDispatch = {
  var kind:String;
  var value:Int;
};
