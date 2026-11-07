package flixel.util;

/**
 * Probe stub for flixel.util.FlxSignal: the retained real Conductor uses the
 * instance signals `onStepHit`/`onBeatHit`/`onMeasureHit` in update() and the
 * static ones via the singleton plumbing. The real module closes over
 * FlxDestroyUtil (an openfl chain the probe does not compile), so the stub
 * keeps the add/remove/dispatch surface the retained code uses; listener call
 * order stays insertion order like the real signal. Recorded in
 * tools/oracle/BEHAVIOR.md.
 */
@:nullSafety
class FlxSignal
{
  var listeners:Array<Void->Void> = [];

  public inline function new() {}

  public inline function add(listener:Void->Void):Void
  {
    listeners.push(listener);
  }

  public inline function addOnce(listener:Void->Void):Void
  {
    listeners.push(listener);
  }

  public inline function remove(listener:Void->Void):Void
  {
    listeners.remove(listener);
  }

  public inline function dispatch():Void
  {
    for (listener in listeners.copy())
    {
      listener();
    }
  }

  public inline function has(listener:Void->Void):Bool
  {
    return listeners.indexOf(listener) != -1;
  }
}
