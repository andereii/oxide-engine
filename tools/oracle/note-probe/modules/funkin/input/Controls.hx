package funkin.input;

/**
 * Probe shadow of funkin.input.Controls. onNoteMiss/ghostNoteMiss read the four
 * NOTE_*_P pressed flags only to build a currently-unused indices array
 * (tools/oracle/NOTE-BEHAVIOR.md section 3: they return false).
 */
@:nullSafety
class Controls
{
  public var NOTE_LEFT_P:Bool = false;
  public var NOTE_DOWN_P:Bool = false;
  public var NOTE_UP_P:Bool = false;
  public var NOTE_RIGHT_P:Bool = false;

  public function new() {}
}
