#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/**
 * Field retention for the REAL funkin.play.scoring.Scoring (PBOT1 surface).
 * The dispatch functions and every PBOT1 constant/body the retained path uses
 * are kept byte-identical; LEGACY/WEEK7 bodies are whitelisted away. The
 * ScoringSystem enum abstract lives at module level and is untouched.
 */
class NoteScoringSlice
{
  static final KEEP:Array<String> = [
    "scoreNote",
    "judgeNote",
    "getMissScore",
    "scoreNotePBOT1",
    "judgeNotePBOT1",
    "PBOT1_MAX_SCORE",
    "PBOT1_SCORING_OFFSET",
    "PBOT1_SCORING_SLOPE",
    "PBOT1_MIN_SCORE",
    "PBOT1_MISS_SCORE",
    "PBOT1_PERFECT_THRESHOLD",
    "PBOT1_MISS_THRESHOLD",
    "PBOT1_KILLER_THRESHOLD",
    "PBOT1_SICK_THRESHOLD",
    "PBOT1_GOOD_THRESHOLD",
    "PBOT1_BAD_THRESHOLD",
    "PBOT1_SHIT_THRESHOLD",
    // The retained scoreNote/judgeNote/getMissScore switches call these.
    // Dropping them would require editing the retained switch, which is forbidden.
    "scoreNoteLEGACY",
    "scoreNoteWEEK7",
    "judgeNoteLEGACY",
    "judgeNoteWEEK7",
    "LEGACY_HIT_WINDOW",
    "LEGACY_SICK_THRESHOLD",
    "LEGACY_GOOD_THRESHOLD",
    "LEGACY_BAD_THRESHOLD",
    "LEGACY_SHIT_THRESHOLD",
    "LEGACY_SICK_SCORE",
    "LEGACY_GOOD_SCORE",
    "LEGACY_BAD_SCORE",
    "LEGACY_SHIT_SCORE",
    "LEGACY_MISS_SCORE",
    "WEEK7_HIT_WINDOW",
    "WEEK7_BAD_THRESHOLD",
    "WEEK7_GOOD_THRESHOLD",
    "WEEK7_SICK_THRESHOLD",
    "WEEK7_MISS_SCORE",
    "WEEK7_SHIT_SCORE",
    "WEEK7_BAD_SCORE",
    "WEEK7_GOOD_SCORE",
    "WEEK7_SICK_SCORE"
  ];

  public static function keepPbot1():Array<Field>
  {
    return Context.getBuildFields().filter(function(field) return KEEP.indexOf(field.name) != -1);
  }
}
#end
