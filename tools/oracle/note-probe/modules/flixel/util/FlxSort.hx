package flixel.util;

/**
 * Probe shadow of flixel.util.FlxSort: constants and the byValues comparator
 * copied VERBATIM from .haxelib/flixel/git/flixel/util/FlxSort.hx:L8-9, L24-40
 * (same disclosure as the proven conductor probe).
 */
@:nullSafety
class FlxSort
{
  public static inline var ASCENDING:Int = -1;
  public static inline var DESCENDING:Int = 1;

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
