package flixel.sound;

/**
 * Probe shadow of the simulated "music" object (same shape as the proven
 * conductor probe). The retained Conductor reads only time/playing/length/pitch.
 */
@:nullSafety
class FlxSound
{
  public var time:Float = 0.0;
  public var length:Float = 0.0;
  public var playing:Bool = false;
  public var pitch:Float = 1.0;

  public function new() {}
}
