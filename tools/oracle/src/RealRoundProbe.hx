import flixel.math.FlxMath;

/** Imports the installed class; ProbeSlice removes only unrelated fields. */
class RealRoundProbe {
  static function main():Void {
    RoundCases.check("real FlxMath.roundDecimal (native hxcpp, unrelated fields sliced)", FlxMath.roundDecimal);
  }
}
