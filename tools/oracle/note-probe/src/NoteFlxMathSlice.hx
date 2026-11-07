#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/** Keep the real FlxMath.roundDecimal body; drop the graphics closure. */
class NoteFlxMathSlice
{
  public static function keepRoundDecimal():Array<Field>
  {
    return Context.getBuildFields().filter(function(field) return field.name == "roundDecimal");
  }
}
#end
