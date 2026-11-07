#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/**
 * ProbeSlice-style field retention for the REAL `funkin.Conductor`, imported
 * unchanged from the installed game source (tools/oracle/BEHAVIOR.md; human
 * approval 2026-11-06). The keep list contains only the fields the native
 * probe exercises (the update/mapTimeChanges timing path and the singleton/
 * dispatch plumbing); every unrelated graphics/gameplay field is removed
 * before typing, exactly like probes/keepRoundDecimal for FlxMath. No field
 * BODY of a retained method is altered by this macro.
 */
class ConductorSlice {
  static final KEEP:Array<String> = [
    // Singleton + dispatch plumbing (used by the retained update() shape).
    "instance",
    "_instance",
    "measureHit",
    "beatHit",
    "stepHit",
    "onMeasureHit",
    "onBeatHit",
    "onStepHit",
    "get_instance",
    "set_instance",
    "clearSingleton",
    "setupSingleton",
    "dispatchMeasureHit",
    "dispatchBeatHit",
    "dispatchStepHit",
    "reset",
    // Timing state and the timing path (rows are read from these).
    "currentTimeChange",
    "songPosition",
    "songPositionDelta",
    "prevTime",
    "prevTimestamp",
    "bpmOverride",
    "timeChanges",
    "bpm",
    "startingBPM",
    "measureLengthMs",
    "beatLengthMs",
    "stepLengthMs",
    "currentMeasure",
    "currentBeat",
    "currentStep",
    "currentMeasureTime",
    "currentBeatTime",
    "currentStepTime",
    "instrumentalOffset",
    "instrumentalOffsetSteps",
    "formatOffset",
    "globalOffset",
    "audioVisualOffset",
    "combinedOffset",
    "beatsPerMeasure",
    "stepsPerMeasure",
    "timeSignatureNumerator",
    "timeSignatureDenominator",
    "get_bpm",
    "get_startingBPM",
    "get_measureLengthMs",
    "get_beatLengthMs",
    "get_stepLengthMs",
    "get_timeSignatureNumerator",
    "get_timeSignatureDenominator",
    "get_instrumentalOffsetSteps",
    "get_globalOffset",
    "get_audioVisualOffset",
    "get_combinedOffset",
    "get_beatsPerMeasure",
    "get_stepsPerMeasure",
    "new",
    "update",
    "mapTimeChanges",
    "getTimeInMeasures",
    "log"
  ];

  public static function keepTimingPath():Array<Field> {
    return Context.getBuildFields().filter(function(field) return KEEP.indexOf(field.name) != -1);
  }
}
#end
