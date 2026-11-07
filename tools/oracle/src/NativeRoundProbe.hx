/** Native Haxe runtime evidence only: this does not import the game or FlxMath. */
class NativeRoundProbe {
  static function main():Void {
    RoundCases.check("native hxcpp Math.fround scaling harness", function(value:Float, precision:Int):Float {
      var mult:Float = 1.0;
      for (i in 0...precision) mult *= 10.0;
      return Math.fround(value * mult) / mult;
    });
  }
}
