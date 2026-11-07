package funkin.modding.events;

/**
 * Probe shadow of ScriptEventDispatcher: no mods/scripts exist in the probe, so
 * callEvent is a shell that leaves the event uncanceled (the retained
 * PlayState.dispatchEvent calls it for song/stage/conversation targets).
 */
class ScriptEventDispatcher
{
  public static function callEvent(target:Dynamic, event:ScriptEvent):Void {}
}
