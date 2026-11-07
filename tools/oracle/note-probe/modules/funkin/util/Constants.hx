package funkin.util;

/**
 * Probe shadow of funkin.util.Constants: only the constant set the retained
 * note path reads. Each value is copied VERBATIM from
 * source/funkin/util/Constants.hx with its source line citation; the unrelated
 * registry-color/application helpers are dropped because they reference lime
 * and the graphics closure (tools/oracle/NOTE-BEHAVIOR.md section 3).
 */
@:nullSafety
class Constants
{
  // Constants.hx:L261
  public static final DEFAULT_BPM:Float = 100.0;
  // Constants.hx:L326/L331 (default time signature)
  public static final DEFAULT_TIME_SIGNATURE_NUM:Int = 4;
  public static final DEFAULT_TIME_SIGNATURE_DEN:Int = 4;
  // Constants.hx:L356
  public static final PIXELS_PER_MS:Float = 0.45;
  // Constants.hx:L361
  public static final HIT_WINDOW_MS:Float = 160.0;
  // Constants.hx:L368
  public static final SECS_PER_MIN:Int = 60;
  // Constants.hx:L373
  public static final MS_PER_SEC:Int = 1000;
  // Constants.hx:L378-393 (US_PER_MS = 1000, US_PER_SEC = 1e6, NS_PER_US = 1000)
  public static final US_PER_MS:Int = 1000;
  public static final NS_PER_US:Int = 1000;
  public static final NS_PER_MS:Int = NS_PER_US * US_PER_MS;
  // Constants.hx:L424
  public static final STEPS_PER_BEAT:Int = 4;
  // Constants.hx:L455
  public static final HEALTH_MAX:Float = 2.0;
  // Constants.hx:L460
  public static final HEALTH_STARTING:Float = 1.0;
  // Constants.hx:L466
  public static final HEALTH_MIN:Float = 0.0;
  // Constants.hx:L476
  public static final HEALTH_SICK_BONUS:Float = 1.5 / 100.0 * HEALTH_MAX;
  // Constants.hx:L481
  public static final HEALTH_GOOD_BONUS:Float = 0.75 / 100.0 * HEALTH_MAX;
  // Constants.hx:L486
  public static final HEALTH_BAD_BONUS:Float = 0.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L492
  public static final HEALTH_SHIT_BONUS:Float = -1.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L497
  public static final HEALTH_HOLD_BONUS_PER_SECOND:Float = 6.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L502
  public static final HEALTH_MISS_PENALTY:Float = -4.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L507
  public static final HEALTH_GHOST_MISS_PENALTY:Float = -4.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L512
  public static final HEALTH_HOLD_DROP_PENALTY_PER_SECOND:Float = 0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L517
  public static final HEALTH_HOLD_DROP_PENALTY_MAX:Float = 0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L522
  public static final HEALTH_MINE_PENALTY:Float = -15.0 / 100.0 * HEALTH_MAX;
  // Constants.hx:L532
  public static final SCORE_GHOST_MISS_PENALTY:Float = -10.0;
  // Constants.hx:L538
  public static final SCORE_HOLD_BONUS_PER_SECOND:Float = 250.0;
  // Constants.hx:L543
  public static final SCORE_HOLD_DROP_PENALTY_PER_SECOND:Float = -125.0;
  // Constants.hx:L548
  public static final HOLD_DROP_PENALTY_THRESHOLD_MS:Float = 160.0;
  // Constants.hx:L558
  public static final JUDGEMENT_SICK_COMBO_BREAK:Bool = false;
  // Constants.hx:L563
  public static final JUDGEMENT_GOOD_COMBO_BREAK:Bool = false;
  // Constants.hx:L568
  public static final JUDGEMENT_BAD_COMBO_BREAK:Bool = true;
  // Constants.hx:L573
  public static final JUDGEMENT_SHIT_COMBO_BREAK:Bool = true;
}
