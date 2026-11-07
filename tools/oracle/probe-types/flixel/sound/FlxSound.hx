package flixel.sound;

/**
 * Probe stub: the simulated "music" object. The real FlxSound is an openfl
 * engine class; the retained Conductor reads only `time`, `playing`, `length`
 * and `pitch` (Conductor.hx L424-425, L439-444). The probe drives these
 * exactly as the frozen recipe does.
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
