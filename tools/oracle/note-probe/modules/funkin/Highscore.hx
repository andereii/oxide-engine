package funkin;

/**
 * Probe shadow of funkin.Highscore: the Tallies state surface the retained
 * applyScore/goodNoteHit bodies touch, with field names/types copied from
 * source/funkin/Highscore.hx:L80-104 (combo, missed, shit, bad, good, sick,
 * maxCombo, totalNotesHit). `tallies` is the real observed state source for
 * the emitted row fields (NOTE-BEHAVIOR.md section 6).
 */
@:nullSafety
class Highscore
{
  public static var tallies:Tallies = new Tallies();
}

class Tallies
{
  public var combo:Int = 0;
  public var missed:Int = 0;
  public var shit:Int = 0;
  public var bad:Int = 0;
  public var good:Int = 0;
  public var sick:Int = 0;
  public var maxCombo:Int = 0;
  public var score:Int = 0;
  public var isNewHighscore:Bool = false;
  public var totalNotesHit:Int = 0;
  public var totalNotes:Int = 0;

  public function new() {}
}
