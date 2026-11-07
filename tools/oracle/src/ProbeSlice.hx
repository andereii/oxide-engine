#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/** Keep the real method body unchanged; avoid typing unrelated graphics methods. */
class ProbeSlice {
  public static function keepRoundDecimal():Array<Field> {
    return Context.getBuildFields().filter(function(field) return field.name == "roundDecimal");
  }
}
#end
