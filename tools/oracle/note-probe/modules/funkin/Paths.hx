package funkin;

/**
 * Probe shadow of funkin.Paths: the retained miss/ghost sound calls resolve
 * `soundRandom`; the probe supplies no audio (call sites are observably
 * excluded from the trace, tools/oracle/NOTE-BEHAVIOR.md section 3).
 */
@:nullSafety
class Paths
{
  public static function soundRandom(base:String, min:Int, max:Int):String
  {
    return '';
  }

  public static function sound(name:String):String
  {
    return '';
  }
}
