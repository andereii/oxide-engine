package funkin;

/**
 * Probe shadow of funkin.Preferences: only `globalOffset` is read by the
 * retained real Conductor (Conductor.hx:L279-281). The probe recipe has no
 * calibration offset (0), matching the frozen synthesized stream.
 */
@:nullSafety
class Preferences
{
  public static var globalOffset:Int = 0;
}
