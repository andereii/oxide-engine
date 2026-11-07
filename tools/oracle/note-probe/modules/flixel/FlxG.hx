package flixel;

import flixel.sound.FlxSound;

/**
 * Probe shadow of flixel.FlxG. The retained note path reads `random.float`
 * (miss/ghost sound volume; audio output itself is excluded), `log`
 * (Scoring/PopUpStuff warnings) and the simulated music object used by the
 * retained Conductor. Engine decision: no engine, no audio, `elapsed` = 0.
 */
@:nullSafety
class FlxG
{
  public static var sound:SoundManager = new SoundManager();
  public static var elapsed:Float = 0.0;
  public static var random:RandomStub = new RandomStub();
  public static var log:LogStub = new LogStub();
}

class SoundManager
{
  public var music:Null<FlxSound> = null;

  public function new() {}
}

class RandomStub
{
  public function new() {}

  public function float(min:Float, max:Float):Float
  {
    return min;
  }
}

class LogStub
{
  public function new() {}

  public function error(message:Dynamic):Void {}

  public function warn(message:Dynamic):Void {}
}
