package funkin.save;

/**
 * Probe shadow of funkin.save.Save. The retained Conductor reads
 * `Save?.instance?.options?.audioVisualOffset ?? 0` (Conductor.hx:L284-286) and
 * Scoring only imports the score-data type names for dropped LEGACY helpers.
 * The probe leaves the save null (no-save state, avOff 0).
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

class SaveScoreData
{
  public function new() {}
}

class SaveScoreTallyData
{
  public function new() {}
}
