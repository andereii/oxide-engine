import funkin.modding.events.ScriptEvent;

/**
 * Runtime bridge between the dispatchEvent instrumentation shell (the
 * MusicBeatSubState shadow) and the driver. One record per dispatch, carrying
 * the pre-mutation health/combo snapshot needed for the §2 `*_before` fields
 * (NOTE-BEHAVIOR.md section 6 flush rule).
 */
class NoteProbeRuntime
{
  public static var events:Array<DispatchRecord> = [];
  public static var healthProvider:Void->Float = null;
  public static var comboProvider:Void->Int = null;

  public static function onDispatch(event:ScriptEvent):Void
  {
    var health = healthProvider != null ? healthProvider() : 0.0;
    var combo = comboProvider != null ? comboProvider() : 0;
    events.push({event: event, health: health, combo: combo});
  }

  public static function take():Array<DispatchRecord>
  {
    var taken = events;
    events = [];
    return taken;
  }

  public static function reset():Void
  {
    events = [];
  }
}

typedef DispatchRecord =
{
  event:ScriptEvent,
  health:Float,
  combo:Int
};
