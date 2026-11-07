package funkin;

/**
 * Probe stub for funkin.Preferences (real file is a registry facade). Only
 * `globalOffset` is read by the retained real Conductor (getter
 * Conductor.hx:L279-281) and it is the user calibration input to the frozen
 * recipe. Defaults to 0 like the observed no-save-game state (BEHAVIOR.md).
 */
@:nullSafety
class Preferences
{
  public static var globalOffset:Int = 0;
}
