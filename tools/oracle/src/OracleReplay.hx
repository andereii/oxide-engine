import haxe.Json;
import OracleAssets.OracleMetadata;
import OracleFixture.OracleSimulation;

/** Recorded simulation inputs/counts; full timeline expectations remain model-only. */
class OracleReplay {
  public static function input(id:String, metadata:OracleMetadata, lengthMs:Float):Dynamic {
    // Copy before mapTimeChanges mutates the model's source objects.
    var changes:Array<Dynamic> = [];
    for (change in metadata.changes) {
      changes.push({t: change.timeStamp, b: change.beatTime, bpm: change.bpm,
        n: change.timeSignatureNum, d: change.timeSignatureDen});
    }
    return {id: id, lengthMs: lengthMs, timeChanges: changes,
      iOff: metadata.instrumentalOffset, fOff: 0.0, avOff: 0.0, gOff: 0.0};
  }

  public static function finish(input:Dynamic, simulation:OracleSimulation):Void {
    input.steps = simulation.steps;
    input.beats = simulation.beats;
    input.measures = simulation.measures;
    input.rows = simulation.lines.length;
  }

  public static function format(charts:Array<Dynamic>, smoke:Dynamic):String {
    return Json.stringify({schema: 1, simulation: "countdown-1ms-playback-1ms-v1",
      reference: "model-only; utility probes documented in tools/oracle/BEHAVIOR.md",
      charts: charts, smoke: smoke}, null, "  ") + "\n";
  }
}
