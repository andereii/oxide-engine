package funkin.play;

/**
 * Probe stub: the real PlayState (huge gameplay class) is shadowed. The
 * retained real Conductor only touches `PlayState.instance.startingSong` /
 * `startTimestamp` inside get_bpm(); the probe leaves instance null and records
 * in tools/oracle/BEHAVIOR.md that the scan selects timeChanges[0] the same
 * way both executed and transcribed paths do for this corpus (startTimestamp 0
 * separator). No original modification.
 */
@:nullSafety
class PlayState
{
  public static var instance:Null<PlayState> = null;
  public var startingSong:Bool = false;
  public var startTimestamp:Float = 0.0;
}
