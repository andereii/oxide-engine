package flixel.util;

/**
 * Probe stub for flixel.util.FlxSort (module reached via the retained real
 * SongDataUtils.sortTimeChanges stub). The real module imports FlxObject, an
 * engine closure unrelated to the retained Conductor path, so the stub keeps
 * the constant values and the `byValues` comparator copied VERBATIM from the
 * installed source (.haxelib/flixel/git/flixel/util/FlxSort.hx:L8-9 constants,
 * L24-40 comparator). Disclosure lives in tools/oracle/BEHAVIOR.md.
 */
@:nullSafety
class FlxSort
{
  public static inline var ASCENDING:Int = -1;
  public static inline var DESCENDING:Int = 1;

  // FlxSort.hx:L24-40 (byValues comparator, verbatim)
  public static inline function byValues(Order:Int, Value1:Float, Value2:Float):Int
  {
    var result:Int = 0;

    if (Value1 < Value2)
    {
      result = Order;
    }
    else if (Value1 > Value2)
    {
      result = -Order;
    }

    return result;
  }
}
