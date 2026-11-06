import haxe.Json;
import sys.FileSystem;
import sys.io.File;
import sys.io.Process;

/**
 * Read-only access to the reference asset tree.
 *
 * V-Slice stores chart/metadata JSON under `assets/preload/data/songs/<id>/`
 * and audio under `assets/songs/<id>/`. The M4 contract describes the JSON
 * path as `assets/songs/<id>/`; both layouts are accepted, with the real
 * preload path preferred. Nothing is ever written below `assets`.
 *
 * Metadata selection mirrors `SongRegistry.loadEntryMetadataFile`
 * (source/funkin/data/song/SongRegistry.hx:L464-477): the default variation
 * uses `<id>-metadata.json`, every other variation uses
 * `<id>-metadata-<variation>.json`.
 */
class OracleAssets {
  public static function resolveDataRoot(assets:String):String {
    var preload = join(join(join(assets, "preload"), "data"), "songs");
    if (FileSystem.exists(preload) && FileSystem.isDirectory(preload)) return preload;

    var songs = join(assets, "songs");
    if (FileSystem.exists(songs) && FileSystem.isDirectory(songs)) return songs;

    throw "no song data directory under " + assets + " (looked for preload/data/songs and songs)";
  }

  /**
   * Enumerates every `<id>-chart[-<variation>].json` file, sorted by fixture id
   * (`<id>-default`, `<id>-<variation>`). The reference tree yields 61 files.
   */
  public static function discoverCharts(assets:String):Array<OracleChart> {
    var dataRoot = resolveDataRoot(assets);
    var songIds = FileSystem.readDirectory(dataRoot);
    songIds.sort(Reflect.compare);

    var charts:Array<OracleChart> = [];
    var seen:Map<String, Bool> = new Map();
    var missingMetadata:Array<String> = [];

    for (songId in songIds) {
      var dir = join(dataRoot, songId);
      if (!FileSystem.isDirectory(dir)) continue;

      var files = FileSystem.readDirectory(dir);
      files.sort(Reflect.compare);
      for (fileName in files) {
        var prefix = songId + "-chart";
        var suffix:String;
        if (fileName == prefix + ".json") {
          suffix = "";
        } else if (StringTools.startsWith(fileName, prefix + "-") && StringTools.endsWith(fileName, ".json")) {
          suffix = fileName.substring(prefix.length, fileName.length - ".json".length);
        } else {
          continue;
        }

        if (suffix == "-") continue;

        var variation = suffix == "" ? "default" : suffix.substring(1);
        var fixtureId = songId + (suffix == "" ? "-default" : suffix);
        if (seen.exists(fixtureId)) throw "duplicate chart file for fixture id " + fixtureId;
        seen.set(fixtureId, true);

        var metadataPath = join(dir, songId + "-metadata" + suffix + ".json");
        if (!FileSystem.exists(metadataPath)) {
          missingMetadata.push(metadataPath);
          continue;
        }

        charts.push({
          songId: songId,
          variation: variation,
          suffix: suffix,
          fixtureId: fixtureId,
          chartPath: join(dir, fileName),
          metadataPath: metadataPath
        });
      }
    }

    if (missingMetadata.length > 0) {
      throw "missing metadata for " + missingMetadata.length + " chart(s): " + missingMetadata.join(", ");
    }

    charts.sort(function(a:OracleChart, b:OracleChart):Int {
      return Reflect.compare(a.fixtureId, b.fixtureId);
    });

    return charts;
  }

  /**
   * Parses the subset of the song metadata that drives the conductor:
   * `timeChanges` (aliases t/b/bpm/n/d) and `offsets.instrumental`.
   *
   * = funkin/data/song/SongData.hx:L157-203 (SongTimeChange aliases/defaults)
   * = funkin/data/song/SongData.hx:L235-282 (SongOffsets.getInstrumentalOffset)
   *
   * `getInstrumentalOffset(null)` returns the plain `instrumental` field; the
   * alternate-instrumental map is intentionally out of scope for M4 (the
   * fixture always probes the default `Inst.ogg`).
   */
  public static function loadMetadata(chart:OracleChart):OracleMetadata {
    var contents:String = File.getContent(chart.metadataPath);
    var json:Dynamic = Json.parse(contents);
    if (json == null) throw chart.metadataPath + ": empty metadata JSON";

    var timeFormat = "ms";
    var rawFormat:Dynamic = Reflect.field(json, "timeFormat");
    if (rawFormat != null) timeFormat = Std.string(rawFormat);
    if (timeFormat != "ms") {
      throw chart.metadataPath + ": unsupported timeFormat '" + timeFormat + "' (the M4 contract freezes milliseconds)";
    }

    var changes:Array<OracleTimeChange> = [];
    var rawChanges:Dynamic = Reflect.field(json, "timeChanges");
    if (rawChanges != null) {
      if (!Std.isOfType(rawChanges, Array)) throw chart.metadataPath + ": timeChanges is not an array";
      var entries:Array<Dynamic> = cast rawChanges;
      var previousBpm:Float = 0.0;
      for (i in 0...entries.length) {
        var entry:Dynamic = entries[i];
        if (entry == null) throw chart.metadataPath + ": timeChanges[" + i + "] is null";

        var ctx = chart.metadataPath + ": timeChanges[" + i + "]";
        var timeStamp = readFloat(Reflect.field(entry, "t"), ctx + ".t");

        var rawBpm:Dynamic = Reflect.field(entry, "bpm");
        var bpm:Float;
        if (rawBpm == null) {
          // SongData.hx:L177-182: bpm is optional after the first entry and
          // defaults to the previous entry's value.
          if (i == 0) throw ctx + ".bpm is required on the first time change";
          bpm = previousBpm;
        } else {
          bpm = readFloat(rawBpm, ctx + ".bpm");
        }
        previousBpm = bpm;

        var num = readOptionalInt(Reflect.field(entry, "n"), 4, ctx + ".n");
        var den = readOptionalInt(Reflect.field(entry, "d"), 4, ctx + ".d");
        var beatTime = readOptionalFloat(Reflect.field(entry, "b"), 0.0, ctx + ".b");

        changes.push(new OracleTimeChange(timeStamp, bpm, num, den, beatTime));
      }
    }

    var instrumentalOffset:Float = 0.0;
    var offsets:Dynamic = Reflect.field(json, "offsets");
    if (offsets != null) {
      var rawInstrumental:Dynamic = Reflect.field(offsets, "instrumental");
      if (rawInstrumental != null) {
        instrumentalOffset = readFloat(rawInstrumental, chart.metadataPath + ": offsets.instrumental");
      }
    }

    return {
      changes: changes,
      instrumentalOffset: instrumentalOffset,
      timeFormat: timeFormat
    };
  }

  /**
   * music.length for a song is the default instrumental (`assets/songs/<id>/Inst.ogg`)
   * duration in milliseconds, probed fresh for every fixture run.
   */
  public static function probeInstrumentalLengthMs(assets:String, songId:String):Float {
    var path = join(join(join(assets, "songs"), songId), "Inst.ogg");
    if (!FileSystem.exists(path)) throw "missing default instrumental: " + path;

    var ffprobe = Sys.getEnv("FFPROBE");
    if (ffprobe == null || ffprobe == "") ffprobe = "ffprobe";

    var process = new Process(ffprobe, [
      "-v", "error",
      "-show_entries", "format=duration",
      "-of", "default=noprint_wrappers=1:nokey=1",
      path
    ]);
    var stdout = process.stdout.readAll().toString();
    var stderr = process.stderr.readAll().toString();
    var code = process.exitCode();
    process.close();

    if (code != 0) {
      throw "ffprobe failed for " + path + " (exit " + code + "): " + StringTools.trim(stderr);
    }

    var seconds = Std.parseFloat(StringTools.trim(stdout));
    if (Math.isNaN(seconds) || seconds <= 0.0) {
      throw "ffprobe returned an invalid duration for " + path + ": " + Std.string(stdout);
    }

    return seconds * 1000.0;
  }

  static function readFloat(value:Dynamic, context:String):Float {
    if (value == null) throw context + ": missing number";

    var parsed = Std.parseFloat(Std.string(value));
    if (Math.isNaN(parsed)) throw context + ": not a number (" + Std.string(value) + ")";

    return parsed;
  }

  static function readOptionalFloat(value:Dynamic, fallback:Float, context:String):Float {
    if (value == null) return fallback;

    return readFloat(value, context);
  }

  static function readOptionalInt(value:Dynamic, fallback:Int, context:String):Int {
    if (value == null) return fallback;

    var parsed = Std.parseInt(Std.string(value));
    if (parsed == null) throw context + ": not an integer (" + Std.string(value) + ")";

    return parsed;
  }

  static function join(left:String, right:String):String {
    if (StringTools.endsWith(left, "/")) return left + right;

    return left + "/" + right;
  }
}

typedef OracleChart = {
  var songId:String;
  var variation:String;
  var suffix:String;
  var fixtureId:String;
  var chartPath:String;
  var metadataPath:String;
};

typedef OracleMetadata = {
  var changes:Array<OracleTimeChange>;
  var instrumentalOffset:Float;
  var timeFormat:String;
};
