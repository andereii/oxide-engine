package funkin.save;

/**
 * Probe stub for funkin.save.Save: the retained real Conductor reads only
 * `Save?.instance?.options?.audioVisualOffset ?? 0` (Conductor.hx:L284-286).
 * The probe leaves both null so the expression evaluates to 0, matching the
 * observed no-save state recorded in the frozen offsets.
 */
@:nullSafety
class Save
{
  public static var instance:Null<Save> = null;
  public var options:Null<SaveOptions> = null;
}

class SaveOptions
{
  public var audioVisualOffset:Int = 0;
}
