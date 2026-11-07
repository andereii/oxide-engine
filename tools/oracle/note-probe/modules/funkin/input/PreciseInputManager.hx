package funkin.input;

import haxe.Int64;
import funkin.play.notes.NoteDirection;

/**
 * Probe shadow of funkin.input.PreciseInputManager. The retained goodNoteHit
 * computes `getCurrentTimestamp() - input.timestamp`; the driver pins both to
 * the same probe value so the latency term is exactly zero (NOTE-BEHAVIOR.md
 * section 1: the synthesized stream has no device latency). Device
 * timestamping is explicitly not covered.
 */
@:nullSafety
class PreciseInputManager
{
  public static var probeTimestamp:Int64 = Int64.ofInt(0);

  public static function getCurrentTimestamp():Int64
  {
    return probeTimestamp;
  }
}

typedef PreciseInputEvent =
{
  noteDirection:NoteDirection,
  timestamp:Int64,
  keyCode:Int
};
