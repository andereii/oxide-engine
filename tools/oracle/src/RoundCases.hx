/** Discriminating utility/runtime evidence; not a full Conductor oracle. */
class RoundCases {
  public static function check(label:String, round:Float->Int->Float):Void {
    Sys.println(label);
    var cases:Array<{value:Float, digits:Int, expected:Float}> = [
      {value: 0.5, digits: 0, expected: 1.0},
      {value: -0.5, digits: 0, expected: 0.0},
      {value: 2.5, digits: 0, expected: 3.0},
      {value: -1.5, digits: 0, expected: -1.0},
      {value: -1.6, digits: 0, expected: -2.0},
      {value: 1.23456789, digits: 6, expected: 1.234568},
      {value: -1.23456789, digits: 6, expected: -1.234568},
      {value: 1.23445, digits: 4, expected: 1.2345},
      {value: -1.23445, digits: 4, expected: -1.2344},
      // A binary32 cast loses the integer's low bit; an Int-returning round overflows.
      {value: 2147483649.25, digits: 0, expected: 2147483649.0}
    ];
    for (entry in cases) {
      var actual = round(entry.value, entry.digits);
      Sys.println(Std.string(entry.value) + " @ " + entry.digits + " -> " + Std.string(actual));
      if (actual != entry.expected) throw "rounding mismatch: expected " + entry.expected + ", got " + actual;
    }
    Sys.println("rounding cases: 10 passed");
  }
}
