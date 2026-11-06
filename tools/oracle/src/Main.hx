import OracleAssets.OracleChart;
import sys.FileSystem;

/**
 * CLI for the committed M4 conductor oracle.
 *
 *   haxe build.hxml
 *   neko /tmp/opencode/oracle-build/oracle.n \
 *     --assets "$HOME/Descargas/fnf/Funkin/assets" \
 *     --out ../../tests/fixtures/conductor
 *
 * `--only` restricts the run to one or more song ids or fixture ids
 * (repeatable, comma-separated). `--citations` prints the audit table.
 * The smoke fixture is always (re)written next to the chart fixtures.
 */
class Main {
  static function main():Void {
    try {
      run(Sys.args());
    } catch (error:Dynamic) {
      Sys.println("oracle: " + Std.string(error));
      Sys.exit(1);
    }
  }

  static function run(args:Array<String>):Void {
    Verbatim.audit();

    var assets:String = null;
    var outDir:String = null;
    var only:Array<String> = [];

    var i:Int = 0;
    while (i < args.length) {
      var arg = args[i];
      switch (arg) {
        case "--assets":
          i++;
          assets = valueAt(args, i, arg);
        case "--out":
          i++;
          outDir = valueAt(args, i, arg);
        case "--only":
          i++;
          var value = valueAt(args, i, arg);
          for (candidate in value.split(",")) {
            if (candidate != "") only.push(candidate);
          }
        case "--citations":
          Verbatim.printAll();
          return;
        case "--help", "-h":
          printUsage();
          return;
        default:
          throw "unknown argument: " + arg;
      }
      i++;
    }

    if (assets == null || outDir == null) {
      printUsage();
      Sys.exit(2);
    }

    var charts = OracleAssets.discoverCharts(assets);
    if (only.length > 0) {
      charts = charts.filter(function(chart:OracleChart):Bool {
        return Lambda.has(only, chart.songId) || Lambda.has(only, chart.fixtureId);
      });
    }
    if (charts.length == 0) throw "no chart files matched";

    ensureDir(outDir);

    // Probe every unique default instrumental before writing anything, so a
    // missing/ffprobe-unreadable file fails the run without partial output.
    var lengths:Map<String, Float> = new Map();
    for (chart in charts) {
      if (!lengths.exists(chart.songId)) {
        lengths.set(chart.songId, OracleAssets.probeInstrumentalLengthMs(assets, chart.songId));
      }
    }

    for (chart in charts) {
      var metadata = OracleAssets.loadMetadata(chart);
      var cachedLength = lengths.get(chart.songId);
      if (cachedLength == null) throw "no probed length for " + chart.songId;
      var lengthMs:Float = cachedLength;

      var result = OracleFixture.simulate(metadata, lengthMs);
      var fixturePath = join(outDir, chart.fixtureId + ".jsonl");
      OracleFixture.writeFile(fixturePath, result.lines);

      Sys.println(chart.fixtureId
        + ": step=" + result.steps
        + " beat=" + result.beats
        + " measure=" + result.measures
        + " rows=" + result.lines.length
        + " length=" + Std.string(lengthMs) + "ms");
    }

    var smoke = OracleFixture.simulateSmoke();
    OracleFixture.writeFile(join(outDir, "_smoke.jsonl"), smoke.lines);
    Sys.println("_smoke"
      + ": step=" + smoke.steps
      + " beat=" + smoke.beats
      + " measure=" + smoke.measures
      + " rows=" + smoke.lines.length
      + " length=4000ms");

    Sys.println("charts: " + charts.length);
    Sys.println("fixtures written: " + (charts.length + 1) + " (" + charts.length + " chart files + _smoke.jsonl)");
  }

  static function valueAt(args:Array<String>, index:Int, option:String):String {
    if (index >= args.length) throw "missing value for " + option;

    return args[index];
  }

  static function printUsage():Void {
    Sys.println("Usage: oracle --assets <game-assets-dir> --out <fixture-dir> [--only <song-id|fixture-id>[,<id>...]] [--citations]");
  }

  static function ensureDir(path:String):Void {
    if (path == "" || path == "/") return;

    if (FileSystem.exists(path)) {
      if (!FileSystem.isDirectory(path)) throw "not a directory: " + path;
      return;
    }

    var parent = haxe.io.Path.directory(path);
    if (parent != null && parent != "" && parent != path) ensureDir(parent);
    FileSystem.createDirectory(path);
  }

  static function join(left:String, right:String):String {
    if (StringTools.endsWith(left, "/")) return left + right;

    return left + "/" + right;
  }
}
