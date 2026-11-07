package flixel;

import flixel.sound.FlxSound;

/**
 * Probe stub for flixel.FlxG (the real engine core is far beyond the retained
 * Conductor surface). Only the fields the real retained Conductor reads exist:
 * FlxG.sound.music (the simulated playback object) and FlxG.elapsed. Engine
 * decision: elapsed is 0 so `songPositionDelta += elapsed * 1000 * pitch`
 * (Conductor.hx L443) is inert — frame deltas are not part of the frozen
 * timeline recipe.
 */
@:nullSafety
class FlxG
{
  public static var sound:SoundManager = new SoundManager();
  public static var elapsed:Float = 0.0;
}

class SoundManager
{
  public var music:Null<FlxSound> = null;

  public function new() {}
}
