/** Ensures the canonical Neko model has the observed native utility semantics. */
class ModelRoundProbe {
  static function main():Void {
    RoundCases.check("model roundDecimal (Neko; native utility evidence is separate)", OracleConductor.roundDecimal);
  }
}
