/**
 * Audit trail for the Haxe transcription in `OracleConductor.hx`.
 *
 * Every behaviour that the oracle copies from the reference implementation is
 * listed here with the exact file and line range used as habitat. The runner
 * calls `audit()` before generating a fixture so that a missing or duplicated
 * citation is a hard error rather than a silent drift. `snapshot_validate.sh`
 * hashes the same ranges against the real game tree, which is read-only.
 *
 * Paths are relative to the root of the reference repository
 * (`~/Descargas/fnf/Funkin` by default, overridable with FUNKIN_DIR).
 */
class Verbatim {
  public static final CONDUCTOR:String = "source/funkin/Conductor.hx";
  public static final SONG_DATA:String = "source/funkin/data/song/SongData.hx";
  public static final SONG_DATA_UTILS:String = "source/funkin/data/song/SongDataUtils.hx";
  public static final CONSTANTS:String = "source/funkin/util/Constants.hx";
  public static final PLAY_STATE:String = "source/funkin/play/PlayState.hx";
  public static final FLX_MATH:String = ".haxelib/flixel/git/flixel/math/FlxMath.hx";
  public static final PREFERENCES:String = "source/funkin/Preferences.hx";

  public static final CITATIONS:Array<Citation> = [
    {file: CONDUCTOR, range: "L21-46", what: "time-signature naming doc (beats are denominator notes)"},
    {file: CONDUCTOR, range: "L96-101", what: "timeChanges array and currentTimeChange"},
    {file: CONDUCTOR, range: "L123-140", what: "get_bpm, including the startingSong scan"},
    {file: CONDUCTOR, range: "L166-171", what: "measureLengthMs = beatLengthMs * tsNum"},
    {file: CONDUCTOR, range: "L176-182", what: "beatLengthMs = (60/bpm)*1000 * (4/tsDen)"},
    {file: CONDUCTOR, range: "L187-192", what: "stepLengthMs = beatLengthMs / STEPS_PER_BEAT"},
    {file: CONDUCTOR, range: "L197-216", what: "time-signature numerator/denominator getters"},
    {file: CONDUCTOR, range: "L218-246", what: "current step/beat/measure and fractional times"},
    {file: CONDUCTOR, range: "L251", what: "instrumentalOffset"},
    {file: CONDUCTOR, range: "L268-299", what: "format/global/audioVisual offsets and combinedOffset"},
    {file: CONDUCTOR, range: "L301-319", what: "beatsPerMeasure and stepsPerMeasure"},
    {file: CONDUCTOR, range: "L422-517", what: "update(): clamp branch, time-change lookup, rounding, dispatch"},
    {file: CONDUCTOR, range: "L549-600", what: "mapTimeChanges(): sort, clamp negative stamps, recompute beatTime"},
    {file: CONDUCTOR, range: "L607-647", what: "getTimeInMeasures(): per-segment accumulation"},
    {file: SONG_DATA, range: "L157-203", what: "SongTimeChange aliases t/b/bpm/n/d/bt and defaults"},
    {file: SONG_DATA, range: "L235-282", what: "SongOffsets and getInstrumentalOffset(null)"},
    {file: SONG_DATA_UTILS, range: "L267-275", what: "sortTimeChanges ascending by timeStamp"},
    {file: CONSTANTS, range: "L261", what: "DEFAULT_BPM = 100"},
    {file: CONSTANTS, range: "L326-331", what: "default time signature 4/4"},
    {file: CONSTANTS, range: "L368", what: "SECS_PER_MIN = 60"},
    {file: CONSTANTS, range: "L373", what: "MS_PER_SEC = 1000"},
    {file: CONSTANTS, range: "L424", what: "STEPS_PER_BEAT = 4"},
    {file: CONSTANTS, range: "L430", what: "MP3_DELAY_MS (web-only; unused on desktop)"},
    {file: CONSTANTS, range: "L618", what: "EXT_SOUND = ogg on desktop => formatOffset 0"},
    {file: PLAY_STATE, range: "L860-883", what: "cache music, set instrumental offset, mapTimeChanges, pre-roll seed update"},
    {file: PLAY_STATE, range: "L1142-1158", what: "countdown update shape and song-start condition"},
    {file: PLAY_STATE, range: "L1172-1180", what: "playback update shape, music.time + combinedOffset"},
    {file: PLAY_STATE, range: "L2634-2665", what: "startSong(): playing=true and music.time = startTimestamp"},
    {file: FLX_MATH, range: "L68-76", what: "roundDecimal = Math.fround(value * mult) / mult (f32 escape, amendment 2026-11-05)"},
    {file: PREFERENCES, range: "L317-322", what: "globalOffset defaults to 0 without a save"}
  ];

  /**
   * Fails when the citation table is empty, contains duplicate entries, or has
   * lost any of the four files that carry the conductor contract itself.
   */
  public static function audit():Void {
    if (CITATIONS.length == 0) throw "Verbatim: citation table is empty";

    var seen:Map<String, Bool> = new Map();
    for (citation in CITATIONS) {
      var key = citation.file + "|" + citation.range;
      if (seen.exists(key)) throw "Verbatim: duplicate citation " + key;
      seen.set(key, true);
    }

    var required = [CONDUCTOR, SONG_DATA, PLAY_STATE, CONSTANTS];
    for (file in required) {
      var count = 0;
      for (citation in CITATIONS) {
        if (citation.file == file) count++;
      }
      if (count == 0) throw "Verbatim: required source has no citations: " + file;
    }
  }

  public static function printAll():Void {
    Sys.println("citation habitat (paths relative to the reference repository):");
    for (citation in CITATIONS) {
      Sys.println("  " + citation.file + " " + citation.range + "  " + citation.what);
    }
  }
}

typedef Citation = {
  var file:String;
  var range:String;
  var what:String;
};
