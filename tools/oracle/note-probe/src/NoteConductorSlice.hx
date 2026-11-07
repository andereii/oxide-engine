#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/**
 * Field retention for the REAL funkin.Conductor. Same timing-path keep list as
 * the proven ConductorSlice plus `getTimeWithDelta` / `startTimestamp`, because
 * the retained GRhythmUtil.getNoteY reads getTimeWithDelta() and the retained
 * Conductor.get_bpm reads PlayState.startTimestamp. Bodies untouched.
 */
class NoteConductorSlice
{
  static final KEEP:Array<String> = [
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
    "getTimeWithDelta",
    "log"
  ];

  public static function keepTimingPath():Array<Field>
  {
    return Context.getBuildFields().filter(function(field) return KEEP.indexOf(field.name) != -1);
  }
}
#end
