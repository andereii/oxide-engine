package flixel.util;

/**
 * Probe shadow of flixel.util.FlxSignal (same shape as the proven conductor
 * probe): insertion-order add/remove/dispatch, enough for Conductor.update's
 * retained dispatch calls.
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
