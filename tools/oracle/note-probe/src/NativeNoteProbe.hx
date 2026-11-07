import flixel.FlxG;
import flixel.sound.FlxSound;
import funkin.Conductor;
import funkin.Highscore;
import funkin.Highscore.Tallies;
import funkin.Preferences;
import funkin.data.song.SongData.SongNoteData;
import funkin.data.song.SongData.SongTimeChange;
import funkin.input.PreciseInputManager;
import funkin.modding.events.ScriptEvent;
import funkin.modding.events.ScriptEvent.NoteScriptEvent;
import funkin.modding.events.ScriptEvent.HitNoteScriptEvent;
import funkin.modding.events.ScriptEvent.GhostMissNoteScriptEvent;
import funkin.modding.events.ScriptEvent.HoldNoteScriptEvent;
import funkin.play.PlayState;
import funkin.play.components.PopUpStuff;
import funkin.play.notes.NoteSprite;
import funkin.play.notes.Strumline;
import funkin.play.notes.SustainTrail;
import funkin.play.scoring.Scoring;
import funkin.util.Constants;
import haxe.Int64;
import sys.io.File;

/**
 * Native hxcpp note-score probe (tools/oracle/NOTE-BEHAVIOR.md).
 *
 * Executes the REAL installed PlayState note-score frame path
 * (processInputQueue -> processNotes, plus goodNoteHit/onNoteMiss/
 * ghostNoteMiss/applyScore), the REAL PBOT1 Scoring and the REAL GRhythmUtil
 * window processing, driven by one `note-stream-1` stream whose Conductor
 * music times are replayed through the REAL retained Conductor.
 *
 * Rows are emitted only for events delivered to PlayState.dispatchEvent
 * (NOTE-BEHAVIOR.md section 6): hit, miss, ghost_press, hold_drop. The engine
 * decision kinds hold_start/hold_tick/hold_end/game_over are NOT dispatched by
 * the retained path and therefore have no native row. Field order is the exact
 * serde order of crates/oxide-game/src/slice.rs TraceEvent; input_time_us is
 * pinned to 0.0.
 *
 * Honesty: synthesized stream; not native-game parity. Excluded: opponent side,
 * audio/visuals, botplay/practice, scripts, input device latency (the latency
 * term is stubbed to exactly 0), runtime clamp/death ordering in update().
 */
@:privateAccess
class NativeNoteProbe
{
  static inline final VERSION = "native-note-probe-1";

  static function main():Void
  {
    var streamPath = envRequired("PROBE_STREAM_JSON");
    var outDir = envRequired("PROBE_OUT_DIR");
    var assetsRoot = envRequired("PROBE_ASSETS");
    Sys.stderr().writeString(honesty());

    var stream = loadStream(streamPath);
    var notes = loadNotes(assetsRoot, stream.difficulty);
    var changes = loadTimeChanges(assetsRoot);

    Conductor.reset();
    var conductor = Conductor.instance;
    conductor.instrumentalOffset = 0.0;
    conductor.formatOffset = 0.0;
    Preferences.globalOffset = 0;
    funkin.save.Save.instance = null;
    FlxG.sound.music = null;
    conductor.mapTimeChanges(changes);

    var ps = buildPlayState(notes);
    if (!sys.FileSystem.exists(outDir))
    {
      sys.FileSystem.createDirectory(outDir);
    }
    var dumpPath = outDir + "/" + stream.difficulty + ".state.jsonl";
    var dump = Sys.getEnv("PROBE_STATE_DUMP") == "1";
    var dumpFile:sys.io.FileOutput = dump ? sys.io.File.write(dumpPath, false) : null;
    var healthExceededMax = false;
    var healthExceededAt = -1;

    var buffer = new StringBuf();
    var rowCount = 0;
    // No conductor update has run yet. P = -1 makes every L632/L721 test fail.
    var previousSongPos:Float = -1.0;

    for (tick in stream.ticks)
    {
      var tickEvents:Array<String> = [];
      // (1) Strumline hold mirror with the PREVIOUS frame's conductor position.
      // PlayState.update calls super.update first (PlayState.hx L1035).
      mirrorHolds(ps, previousSongPos, tick.sim);

      // (2) Conductor update to this tick (PlayState.hx L1142-1189).
      conductor.update(tick.music_ms, false);
      if (conductor.songPosition != tick.sim * 1.0)
      {
        fail('tick ${tick.sim}: retained Conductor.songPosition '
          + Std.string(conductor.songPosition)
          + ' does not equal the exported slice position ${tick.sim}; the stream does not replay the Rust state');
      }
      previousSongPos = conductor.songPosition;

      for (ev in tick.events)
      {
        var input:funkin.input.PreciseInputManager.PreciseInputEvent = {
          noteDirection: ev.direction,
          timestamp: Int64.ofInt(0),
          keyCode: ev.direction
        };
        if (ev.kind == "press")
        {
          @:privateAccess ps.inputPressQueue.push(input);
        }
        else
        {
          @:privateAccess ps.inputReleaseQueue.push(input);
          notes.lastReleaseMs[ev.direction] = ev.input_ms;
        }
      }

      // (3) processInputQueue. Releases update held for the NEXT mirror.
      @:privateAccess ps.processInputQueue();
      markNewHits(ps, tick.sim);
      checkHealth(ps, tick.sim);
      var inputDispatches = flushDispatches(ps, conductor, tick, notes, buffer, tickEvents, null);

      // Snapshot the accrual branch will observe (PlayState.hx L2931). A hit
      // this tick still has the hitNote capture; an older hold has this tick's
      // mirror value.
      var accrued = accruingHolds(ps);
      var comboBeforeNotes = Highscore.tallies.combo;

      // (4) processNotes. A drop flagged by this tick's mirror is penalized here.
      @:privateAccess ps.processNotes(1.0 / 1000.0);
      checkHealth(ps, tick.sim);
      var holdCaptures:Array<NoteProbeDispatch> = [];
      var noteDispatches = flushDispatches(ps, conductor, tick, notes, buffer, tickEvents, holdCaptures);

      if (tickEvents.length > 1)
      {
        fail('multi-event tick sim=${tick.sim}: ' + tickEvents.join("; ")
          + ' — at most one stateful dispatching event per tick has a defensible '
          + 'per-event decomposition (NOTE-BEHAVIOR.md section 6); refusing to emit rows');
      }

      rowCount += inputDispatches + noteDispatches;
      rowCount += emitHoldRows(ps, tick.sim, notes, accrued, comboBeforeNotes, holdCaptures, buffer);
      if (ps.health > Constants.HEALTH_MAX && !healthExceededMax)
      {
        healthExceededMax = true;
        healthExceededAt = tick.sim;
      }
      if (dumpFile != null)
      {
        dumpFile.writeString(stateLine(ps, tick.sim, notes));
      }
    }
    if (dumpFile != null)
    {
      dumpFile.close();
    }
    if (healthExceededMax)
    {
      Sys.stderr().writeString('native-note-probe: health exceeded HEALTH_MAX='
        + Std.string(Constants.HEALTH_MAX) + ' starting at sim ' + healthExceededAt
        + '; update() clamp is not retained and was not emulated\n');
    }

    if (!sys.FileSystem.exists(outDir))
    {
      sys.FileSystem.createDirectory(outDir);
    }
    var outPath = outDir + "/" + stream.difficulty + ".jsonl";
    File.saveContent(outPath, buffer.toString());
    Sys.stderr().writeString('native-note-probe: difficulty ${stream.difficulty}; ticks ${stream.ticks.length}; '
      + 'rows ${rowCount}; wrote ${outPath}\n');
    Sys.stderr().writeString(honesty());
  }

  static function honesty():String
  {
    return 'native-note-probe honesty: synthesized note-stream-1 input; NOT native-game parity; '
      + 'driver hold rows follow NOTE-BEHAVIOR.md section 6.1 round-3 (mirror on previous songPos before the conductor update); '
      + 'game_over is not retained; opponent side, audio/visuals, botplay/practice, scripts and '
      + 'input-device timestamping excluded; input latency term stubbed to 0; input_time_us = 0.0 '
      + 'canonical; hold_drop input_ms = 0.0; defines: NO_FEATURE_GHOST_TAPPING\n';
  }

  static function envRequired(name:String):String
  {
    var value = Sys.getEnv(name);
    if (value == null || value.length == 0)
    {
      fail('required environment variable ' + name + ' is not set');
      return "";
    }
    return value;
  }

  static function fail(message:String):Void
  {
    Sys.stderr().writeString('native-note-probe: FAIL: ' + message + '\n');
    Sys.exit(2);
  }

  // ------------------------------------------------------------------------
  // Stream parsing / fail-closed validation
  // ------------------------------------------------------------------------

  static function loadStream(path:String):ProbeStream
  {
    var text = try
    {
      File.getContent(path);
    }
    catch (error:Dynamic)
    {
      fail('cannot read stream file ' + path + ': ' + Std.string(error));
      return null;
    }
    var rawLines = text.split("\n");
    if (rawLines.length > 0 && rawLines[rawLines.length - 1] == "")
    {
      rawLines.pop();
    }
    if (rawLines.length == 0)
    {
      fail('stream ' + path + ' is empty');
    }

    var header:Dynamic;
    try
    {
      header = haxe.Json.parse(rawLines[0]);
    }
    catch (error:Dynamic)
    {
      fail('stream header is not valid JSON: ' + Std.string(error));
      return null;
    }
    var schema = reqString(header, "schema", "stream header");
    if (schema != "note-stream-1")
    {
      fail('stream header schema is "' + schema + '", expected "note-stream-1"; refusing schema drift');
    }
    var difficulty = reqString(header, "difficulty", "stream header");
    if (difficulty != "easy" && difficulty != "normal" && difficulty != "hard")
    {
      fail('stream header difficulty "' + difficulty + '" is not easy|normal|hard');
    }
    var chartId = reqString(header, "chart_id", "stream header");
    if (chartId != "bopeebo-default")
    {
      fail('stream header chart_id "' + chartId + '", expected "bopeebo-default"');
    }
    var declaredTicks = reqInt(header, "tick_count", "stream header");
    if (declaredTicks < 0)
    {
      fail('stream header tick_count ' + declaredTicks + ' is negative');
    }
    var source = reqString(header, "songpos_source", "stream header");
    if (StringTools.trim(source).length == 0)
    {
      fail('stream header songpos_source is blank');
    }

    var ticks:Array<ProbeTick> = [];
    var lastSim = -1;
    for (index in 1...rawLines.length)
    {
      var line = rawLines[index];
      if (StringTools.trim(line).length == 0)
      {
        fail('stream tick line ' + (index + 1) + ' is blank; fail-closed');
      }
      var tick:Dynamic;
      try
      {
        tick = haxe.Json.parse(line);
      }
      catch (error:Dynamic)
      {
        fail('stream tick line ' + (index + 1) + ' is not valid JSON: ' + Std.string(error));
        return null;
      }
      var sim = reqInt(tick, "sim", "tick line " + (index + 1));
      if (lastSim != -1 && sim != lastSim + 1)
      {
        fail('stream tick line ' + (index + 1) + ' carries sim ' + sim + ' after ' + lastSim
          + '; ticks must be contiguous (absent/short/extra ticks fail closed)');
      }
      if (lastSim == -1 && sim != 0)
      {
        fail('stream first tick carries sim ' + sim + '; ticks must start at 0');
      }
      var music = reqFloat(tick, "music_ms", "tick line " + (index + 1));
      if (music != sim * 1.0)
      {
        fail('tick ' + sim + ' music_ms ' + Std.string(music)
          + ' is not the exact exported slice position ' + sim);
      }
      var eventsRaw:Array<Dynamic> = reqArray(tick, "events", "tick " + sim);
      var events:Array<ProbeInput> = [];
      for (evRaw in eventsRaw)
      {
        var kind = reqString(evRaw, "kind", "tick " + sim + " event");
        if (kind != "press" && kind != "release")
        {
          fail('tick ' + sim + ' event kind "' + kind + '" is not press|release');
        }
        var direction = reqInt(evRaw, "direction", "tick " + sim + " event");
        if (direction < 0 || direction > 3)
        {
          fail('tick ' + sim + ' event direction ' + direction
            + ' is outside the player lanes 0..=3 (opponent/invalid directions are excluded from the frozen stream)');
        }
        var inputMs = reqInt(evRaw, "input_ms", "tick " + sim + " event");
        if (inputMs < sim)
        {
          fail('tick ' + sim + ' event input_ms ' + inputMs + ' is below the tick sim; the export is not on its declared tick');
        }
        if (inputMs != sim)
        {
          fail('tick ' + sim + ' event input_ms ' + inputMs + ' does not equal the tick sim');
        }
        events.push({kind: kind, direction: direction, input_ms: inputMs});
      }
      ticks.push({sim: sim, music_ms: music, events: events});
      lastSim = sim;
    }
    if (ticks.length != declaredTicks)
    {
      fail('stream has ' + ticks.length + ' tick lines but the header declares ' + declaredTicks
        + '; absent/short/extra streams fail closed');
    }
    return {difficulty: difficulty, tick_count: declaredTicks, ticks: ticks};
  }

  static function reqString(value:Dynamic, key:String, context:String):String
  {
    var field = Reflect.field(value, key);
    if (!Std.isOfType(field, String))
    {
      fail(context + ' lacks a string ' + key);
    }
    return cast field;
  }

  static function reqInt(value:Dynamic, key:String, context:String):Int
  {
    var field = Reflect.field(value, key);
    if (Std.isOfType(field, Int))
    {
      return cast field;
    }
    if (Std.isOfType(field, Float))
    {
      var number:Float = cast field;
      if (number == Math.floor(number) && Math.abs(number) <= 2147483647.0)
      {
        return Std.int(number);
      }
    }
    fail(context + ' lacks an integer ' + key);
    return 0;
  }

  static function reqFloat(value:Dynamic, key:String, context:String):Float
  {
    var field = Reflect.field(value, key);
    if (Std.isOfType(field, Int) || Std.isOfType(field, Float))
    {
      var number:Float = cast field;
      if (!Math.isFinite(number))
      {
        fail(context + ' has a non-finite ' + key);
      }
      return number;
    }
    fail(context + ' lacks a number ' + key);
    return 0.0;
  }

  static function reqArray(value:Dynamic, key:String, context:String):Array<Dynamic>
  {
    var field = Reflect.field(value, key);
    if (!Std.isOfType(field, Array))
    {
      fail(context + ' lacks an array ' + key);
    }
    return cast field;
  }

  // ------------------------------------------------------------------------
  // Chart / metadata / state construction
  // ------------------------------------------------------------------------

  static function loadTimeChanges(assetsRoot:String):Array<SongTimeChange>
  {
    var path = assetsRoot + "/preload/data/songs/bopeebo/bopeebo-metadata.json";
    var metadata:Dynamic;
    try
    {
      metadata = haxe.Json.parse(File.getContent(path));
    }
    catch (error:Dynamic)
    {
      fail('cannot read/parse bopeebo metadata ' + path + ': ' + Std.string(error));
      return null;
    }
    var rawChanges:Array<Dynamic> = reqArray(metadata, "timeChanges", "bopeebo metadata");
    if (rawChanges.length == 0)
    {
      fail('bopeebo metadata has no timeChanges');
    }
    var changes:Array<SongTimeChange> = [];
    for (raw in rawChanges)
    {
      var t = reqFloat(raw, "t", "timeChange");
      var bpm = reqFloat(raw, "bpm", "timeChange");
      var n = 4;
      var d = 4;
      if (Reflect.hasField(raw, "n") && Reflect.field(raw, "n") != null) n = reqInt(raw, "n", "timeChange");
      if (Reflect.hasField(raw, "d") && Reflect.field(raw, "d") != null) d = reqInt(raw, "d", "timeChange");
      var beatTime:Null<Float> = null;
      if (Reflect.hasField(raw, "b") && Reflect.field(raw, "b") != null) beatTime = reqFloat(raw, "b", "timeChange");
      changes.push(new SongTimeChange(t, bpm, n, d, beatTime));
    }
    return changes;
  }

  static function loadNotes(assetsRoot:String, difficulty:String):LoadedNotes
  {
    var path = assetsRoot + "/preload/data/songs/bopeebo/bopeebo-chart.json";
    var chart:Dynamic;
    try
    {
      chart = haxe.Json.parse(File.getContent(path));
    }
    catch (error:Dynamic)
    {
      fail('cannot read/parse bopeebo chart ' + path + ': ' + Std.string(error));
      return null;
    }
    var notesByDifficulty:Dynamic = Reflect.field(chart, "notes");
    if (notesByDifficulty == null)
    {
      fail('bopeebo chart has no notes object');
    }
    var rawNotes:Array<Dynamic> = cast Reflect.field(notesByDifficulty, difficulty);
    if (rawNotes == null)
    {
      fail('bopeebo chart has no "' + difficulty + '" difficulty');
    }

    var notes:Array<NoteSprite> = [];
    var holds:Array<SustainTrail> = [];
    var holdNoteIds:Array<Int> = [];
    var sentinel:Dynamic = {};
    for (index in 0...rawNotes.length)
    {
      var raw = rawNotes[index];
      var t = reqFloat(raw, "t", "chart note " + index);
      var d = reqInt(raw, "d", "chart note " + index);
      if (d < 0 || d > 7)
      {
        fail('chart note ' + index + ' direction ' + d + ' is outside the V-Slice 0..=7 lane range');
      }
      if (d > 3)
      {
        // Opponent-side notes are excluded from the phase-1 player trace.
        continue;
      }
      if (!Math.isFinite(t) || t < 0.0)
      {
        fail('chart note ' + index + ' time ' + Std.string(t) + ' must be finite and non-negative');
      }
      var length = 0.0;
      if (Reflect.hasField(raw, "l") && Reflect.field(raw, "l") != null)
      {
        length = reqFloat(raw, "l", "chart note " + index);
      }
      if (!Math.isFinite(length) || length < 0.0)
      {
        fail('chart note ' + index + ' sustain ' + Std.string(length) + ' must be finite and non-negative');
      }
      var kind = "";
      if (Reflect.hasField(raw, "k") && Reflect.field(raw, "k") != null)
      {
        kind = reqString(raw, "k", "chart note " + index);
      }

      var data = new SongNoteData(t, d, length, kind);
      var note = new NoteSprite();
      // The real NoteSprite.direction setter short-circuits when frames is null;
      // the probe keeps frames null at rest ([D]) and uses a temporary sentinel
      // only while assigning the chart direction, so the verbatim setter body runs.
      note.frames = sentinel;
      note.direction = d;
      note.frames = null;
      note.noteData = data;
      note.scoreable = true;
      note.alive = true;
      notes.push(note);

      if (length > 0.0)
      {
        var hold = new SustainTrail();
        hold.strumTime = t;
        hold.noteDirection = d;
        hold.sustainLength = length;
        hold.fullSustainLength = length;
        hold.noteData = data;
        hold.scoreable = true;
        hold.alive = true;
        note.holdNoteSprite = hold;
        holds.push(hold);
        holdNoteIds.push(notes.length - 1);
      }
    }
    if (notes.length == 0)
    {
      fail('bopeebo ' + difficulty + ' holds no player notes');
    }
    return {notes: notes, holds: holds, holdNoteIds: holdNoteIds, lastReleaseMs: [-1, -1, -1, -1]};
  }

  static function buildPlayState(loaded:LoadedNotes):PlayState
  {
    var ps:PlayState = Type.createEmptyInstance(PlayState);
    PlayState.instance = ps;
    @:privateAccess ps.startingSong = false;
    ps.startTimestamp = 0.0;

    ps.health = Constants.HEALTH_STARTING;
    ps.songScore = 0.0;
    ps.isPracticeMode = false;
    ps.isBotPlayMode = false;
    ps.isInCutscene = false;
    ps.disableKeys = false;

    @:privateAccess ps.inputPressQueue = [];
    @:privateAccess ps.inputReleaseQueue = [];

    ps.playerStrumline = new Strumline();
    ps.playerStrumline.isPlayer = true;
    ps.opponentStrumline = new Strumline();
    ps.opponentStrumline.isPlayer = false;

    for (note in loaded.notes)
    {
      ps.playerStrumline.notes.members.push(note);
    }
    for (hold in loaded.holds)
    {
      ps.playerStrumline.holdNotes.members.push(hold);
    }

    ps.vocals = null;
    ps.playerVocalsVolume = 1.0;
    ps.opponentVocalsVolume = 1.0;
    ps.currentStage = null;
    ps.currentSong = null;
    @:privateAccess ps.songEvents = [];
    ps.currentConversation = null;
    ps.comboPopUps = new PopUpStuff();

    Highscore.tallies = new Tallies();
    NoteProbeRuntime.reset();
    NoteProbeRuntime.healthProvider = function() return ps.health;
    NoteProbeRuntime.comboProvider = function() return Highscore.tallies.combo;
    return ps;
  }

  static function markNewHits(ps:PlayState, sim:Int):Void
  {
    for (hold in ps.playerStrumline.holdNotes.members)
    {
      if (hold == null || !hold.hitNote || hold.probeHitSim >= 0) continue;
      hold.probeHitSim = sim;
    }
  }

  /** Holds the retained accrual branch (PlayState.hx L2931-2935) will enter. */
  static function accruingHolds(ps:PlayState):Array<SustainTrail>
  {
    var accrued:Array<SustainTrail> = [];
    if (ps.isBotPlayMode) return accrued;
    for (hold in ps.playerStrumline.holdNotes.members)
    {
      if (hold == null || !hold.alive || hold.probeCompleted) continue;
      if (hold.hitNote && !hold.missedNote && hold.sustainLength > 0 && hold.scoreable)
      {
        hold.probeAccrualSnapshot = hold.sustainLength;
        accrued.push(hold);
      }
    }
    return accrued;
  }

  /**
   * Strumline hold block as the first step of the tick, using P = the previous
   * frame's conductor position (PlayState.hx L1035 super.update before the
   * conductor update). Chain order is Strumline.hx L632-745. Render-only
   * branches (L647-651 offscreen kill, position/alpha/cover) are excluded.
   */
  static function mirrorHolds(ps:PlayState, previousSongPos:Float, sim:Int):Void
  {
    for (hold in ps.playerStrumline.holdNotes.members)
    {
      if (hold == null || !hold.alive || hold.probeCompleted) continue;
      var end = hold.strumTime + hold.fullSustainLength;

      // L632-642 drop detect runs first, including a release at exactly end.
      // The flag does not rewrite the snapshot (§6.1 item 9).
      if (previousSongPos > hold.strumTime && hold.hitNote && !hold.missedNote
        && ps.playerStrumline.isPlayer && !ps.playerStrumline.isKeyHeld(hold.noteDirection))
      {
        hold.missedNote = true;
        hold.probeDropFlagSim = sim;
        hold.probeDropSnapshot = hold.sustainLength;
      }

      // L653-684 completion on the pre-recompute snapshot. This wins over a
      // flag set above when the natural decay has already reached <= 0
      // (release at R >= end+1). No drop row in that case.
      if (hold.hitNote && hold.sustainLength <= 0)
      {
        hold.probeDropFlagSim = -1;
        hold.probeCompleted = true;
        hold.probeEndSim = sim;
        hold.alive = false;
        continue;
      }

      // L685-686 keep-clipped. The drop snapshot is not rewritten.
      if (hold.missedNote && hold.fullSustainLength > hold.sustainLength)
      {
        continue;
      }

      // L721-727 else recompute against P, not against this tick's songPosition.
      if (previousSongPos > hold.strumTime && hold.hitNote && !hold.missedNote)
      {
        hold.sustainLength = end - previousSongPos;
      }
    }
  }

  static function emitHoldRows(ps:PlayState, sim:Int, loaded:LoadedNotes, accrued:Array<SustainTrail>, comboBeforeNotes:Int,
      captures:Array<NoteProbeDispatch>, buffer:StringBuf):Int
  {
    var ordered = playerHolds(ps);
    ordered.sort(function(a:SustainTrail, b:SustainTrail):Int
    {
      if (a.noteDirection != b.noteDirection) return a.noteDirection - b.noteDirection;
      return noteIdOf(loaded, a) - noteIdOf(loaded, b);
    });
    var count = 0;
    for (hold in ordered)
    {
      if (hold.probeHitSim == sim && hold.fullSustainLength > 0)
      {
        buffer.add(holdStartRow(loaded, hold, sim));
        buffer.add("\n");
        count++;
      }
      if (accrued.indexOf(hold) >= 0)
      {
        buffer.add(holdTickRow(loaded, hold, sim));
        buffer.add("\n");
        count++;
      }
      if (hold.probeDropFlagSim == sim && !hold.probeDropEmitted)
      {
        buffer.add(holdDropRow(loaded, hold, sim, comboBeforeNotes, captures));
        buffer.add("\n");
        hold.probeDropEmitted = true;
        count++;
      }
      if (hold.probeEndSim == sim)
      {
        buffer.add(holdEndRow(loaded, hold, sim));
        buffer.add("\n");
        count++;
      }
    }
    return count;
  }

  static function playerHolds(ps:PlayState):Array<SustainTrail>
  {
    var holds:Array<SustainTrail> = [];
    for (hold in ps.playerStrumline.holdNotes.members)
    {
      if (hold != null) holds.push(hold);
    }
    return holds;
  }

  static function holdStartRow(loaded:LoadedNotes, hold:SustainTrail, sim:Int):String
  {
    return header("hold_start", sim, sim * 1.0, noteIdOf(loaded, hold), hold.noteDirection, "none", 0.0, 0) + '}';
  }

  static function holdTickRow(loaded:LoadedNotes, hold:SustainTrail, sim:Int):String
  {
    // hold_remaining_ms is the snapshot the accrual branch observed, not end-T.
    var healthDelta = Constants.HEALTH_HOLD_BONUS_PER_SECOND / 1000.0;
    var scoreDelta = Constants.SCORE_HOLD_BONUS_PER_SECOND / 1000.0;
    return header("hold_tick", sim, 0.0, noteIdOf(loaded, hold), hold.noteDirection, "none", 0.0, 0)
      + ',"health_delta":' + fnum(healthDelta)
      + ',"score_delta":' + fnum(scoreDelta)
      + ',"hold_remaining_ms":' + fnum(hold.probeAccrualSnapshot)
      + ',"hold_elapsed_ms":' + fnum(sim - hold.probeHitSim) + '}';
  }

  static function holdDropRow(loaded:LoadedNotes, hold:SustainTrail, sim:Int, comboBeforeNotes:Int,
      captures:Array<NoteProbeDispatch>):String
  {
    var snapshot = hold.probeDropSnapshot;
    var penalizing = snapshot > Constants.HOLD_DROP_PENALTY_THRESHOLD_MS;
    var capture = findHoldCapture(captures, hold);
    if (penalizing)
    {
      if (capture == null)
      {
        fail('tick ' + sim + ': snapshot ' + fnum(snapshot)
          + ' is above 160 but the retained penalty branch dispatched no HoldNoteScriptEvent');
      }
      var event:HoldNoteScriptEvent = cast capture.event;
      var expected = -125.0 * snapshot / 1000.0;
      if (Math.abs(event.score - expected) > 1e-9 || Math.abs(event.healthChange) > 1e-9 || !event.isComboBreak)
      {
        fail('tick ' + sim + ': HoldNoteScriptEvent score=' + fnum(event.score)
          + ' healthChange=' + fnum(event.healthChange)
          + ' isComboBreak=' + boolJson(event.isComboBreak)
          + ' does not match driver formula -125.0 * snapshot / 1000 = ' + fnum(expected)
          + ' (snapshot ' + fnum(snapshot) + ')');
      }
      if (Highscore.tallies.combo != 0)
      {
        fail('tick ' + sim + ': penalizing hold_drop did not reset combo');
      }
      return header("hold_drop", sim, 0.0, noteIdOf(loaded, hold), hold.noteDirection, "none", 0.0, 0)
        + ',"score_penalty":' + fnum(event.score)
        + ',"health_change":' + fnum(event.healthChange)
        + ',"combo_before":' + Std.string(capture.combo)
        + ',"combo_after":' + Std.string(Highscore.tallies.combo)
        + ',"is_combo_break":' + boolJson(event.isComboBreak) + '}';
    }
    if (capture != null)
    {
      fail('tick ' + sim + ': sub-threshold snapshot ' + fnum(snapshot) + ' still dispatched a HoldNoteScriptEvent');
    }
    if (Highscore.tallies.combo != comboBeforeNotes)
    {
      fail('tick ' + sim + ': sub-threshold hold_drop changed combo from ' + comboBeforeNotes
        + ' to ' + Highscore.tallies.combo);
    }
    return header("hold_drop", sim, 0.0, noteIdOf(loaded, hold), hold.noteDirection, "none", 0.0, 0)
      + ',"score_penalty":' + fnum(0.0)
      + ',"health_change":' + fnum(0.0)
      + ',"combo_before":' + Std.string(comboBeforeNotes)
      + ',"combo_after":' + Std.string(Highscore.tallies.combo)
      + ',"is_combo_break":false}';
  }

  static function findHoldCapture(captures:Array<NoteProbeDispatch>, hold:SustainTrail):NoteProbeDispatch
  {
    for (capture in captures)
    {
      var event:HoldNoteScriptEvent = cast capture.event;
      if (event.holdNote == hold) return capture;
    }
    return null;
  }

  static function holdEndRow(loaded:LoadedNotes, hold:SustainTrail, sim:Int):String
  {
    return header("hold_end", sim, 0.0, noteIdOf(loaded, hold), hold.noteDirection, "none", 0.0, 0) + '}';
  }

  static function noteIdOf(loaded:LoadedNotes, hold:SustainTrail):Int
  {
    var index = loaded.holds.indexOf(hold);
    if (index < 0)
    {
      fail('hold row references a sustain outside the driven player-hold list');
    }
    return loaded.holdNoteIds[index];
  }

  static function checkHealth(ps:PlayState, sim:Int):Void
  {
    if (ps.health <= Constants.HEALTH_MIN)
    {
      fail('death path not retained (health ' + fnum(ps.health) + ' at sim ' + sim + ')');
    }
  }

  static function stateLine(ps:PlayState, sim:Int, loaded:LoadedNotes):String
  {
    var holds = new StringBuf();
    holds.add("[");
    var first = true;
    for (hold in ps.playerStrumline.holdNotes.members)
    {
      if (hold == null || !hold.alive || hold.probeCompleted) continue;
      if (!first) holds.add(",");
      first = false;
      holds.add('{"note_id":' + noteIdOf(loaded, hold)
        + ',"direction":' + hold.noteDirection
        + ',"sustain_length":' + fnum(hold.sustainLength)
        + ',"hit":' + boolJson(hold.hitNote)
        + ',"missed":' + boolJson(hold.missedNote)
        + ',"hit_sim":' + hold.probeHitSim + '}');
    }
    holds.add("]");
    return '{"sim":' + sim
      + ',"health_after":' + fnum(ps.health)
      + ',"song_score":' + fnum(ps.songScore)
      + ',"combo":' + Highscore.tallies.combo
      + ',"tallies":' + talliesJson()
      + ',"active_holds":' + holds.toString() + '}\n';
  }

  // ------------------------------------------------------------------------
  // Row emission (dispatchEvent instrumentation + section 6 flush rule)
  // ------------------------------------------------------------------------

  static function flushDispatches(ps:PlayState, conductor:Conductor, tick:ProbeTick, loaded:LoadedNotes, buffer:StringBuf,
      tickEvents:Array<String>, holdCaptures:Array<NoteProbeDispatch>):Int
  {
    var records = NoteProbeRuntime.take();
    var emitted = 0;
    for (record in records)
    {
      if (record.event.eventCanceled)
      {
        fail('tick ' + tick.sim + ': a script event was canceled; the probe has no scripts, so this is a contract violation');
      }
      tickEvents.push(describeEvent(record.event));
      if (Std.isOfType(record.event, HoldNoteScriptEvent))
      {
        if (holdCaptures == null)
        {
          fail('tick ' + tick.sim + ': HoldNoteScriptEvent dispatched outside processNotes');
        }
        holdCaptures.push(record);
        continue;
      }
      buffer.add(buildRow(ps, conductor, tick, loaded, record));
      buffer.add("\n");
      emitted++;
    }
    return emitted;
  }

  static function describeEvent(event:ScriptEvent):String
  {
    var name = Type.getClassName(Type.getClass(event));
    return name + "(" + Std.string(event.type) + ")";
  }

  static function buildRow(ps:PlayState, conductor:Conductor, tick:ProbeTick, loaded:LoadedNotes, record:NoteProbeDispatch):String
  {
    var event = record.event;
    if (Std.isOfType(event, HitNoteScriptEvent))
    {
      var hit:HitNoteScriptEvent = cast event;
      var index = loaded.notes.indexOf(hit.note);
      if (index < 0)
      {
        fail('tick ' + tick.sim + ': hit event references a note outside the driven player-note list');
      }
      var note = hit.note;
      var rawDiff = conductor.songPosition - note.noteData.time;
      var eventDiff = Std.int(hit.hitDiff);
      if (Std.int(rawDiff) != eventDiff)
      {
        fail('tick ' + tick.sim + ': reconstructed raw note diff ' + Std.string(rawDiff)
          + ' truncates to ' + Std.int(rawDiff) + ' but the real HitNoteScriptEvent carries ' + eventDiff
          + '; the zero-latency reconstruction is inconsistent');
      }
      var judgement = hit.judgement;
      var score = Std.int(hit.score);
      return header("hit", tick.sim, tick.sim * 1.0, index, note.direction, judgement, rawDiff, eventDiff)
        + ',"score_delta":' + Std.string(score)
        + ',"health_before":' + fnum(record.health)
        + ',"health_after":' + fnum(ps.health)
        + ',"combo_before":' + Std.string(record.combo)
        + ',"combo_after":' + Std.string(Highscore.tallies.combo)
        + ',"combo_max":' + Std.string(Highscore.tallies.maxCombo)
        + ',"tallies":' + talliesJson() + '}';
    }
    if (Std.isOfType(event, GhostMissNoteScriptEvent))
    {
      var ghost:GhostMissNoteScriptEvent = cast event;
      return header("ghost_press", tick.sim, tick.sim * 1.0, 0, ghost.dir, "none", 0.0, 0)
        + ',"score_delta":' + fnum(ghost.scoreChange)
        + ',"health_change":' + fnum(ghost.healthChange) + '}';
    }
    if (Std.isOfType(event, HoldNoteScriptEvent))
    {
      var drop:HoldNoteScriptEvent = cast event;
      var hold = drop.holdNote;
      var index = loaded.holds.indexOf(hold);
      if (index < 0)
      {
        fail('tick ' + tick.sim + ': hold_drop event references a hold outside the driven player-hold list');
      }
      var noteIndex = loaded.holdNoteIds[index];
      var direction = hold.noteDirection;
      var inputMs = loaded.lastReleaseMs[direction] != -1 ? loaded.lastReleaseMs[direction] : tick.sim;
      return header("hold_drop", tick.sim, inputMs * 1.0, noteIndex, direction, "none", 0.0, 0)
        + ',"score_penalty":' + fnum(drop.score)
        + ',"health_change":' + fnum(drop.healthChange)
        + ',"combo_before":' + Std.string(record.combo)
        + ',"combo_after":' + Std.string(Highscore.tallies.combo)
        + ',"is_combo_break":' + boolJson(drop.isComboBreak) + '}';
    }
    if (Std.isOfType(event, NoteScriptEvent) && event.type == ScriptEventType.NOTE_MISS)
    {
      var miss:NoteScriptEvent = cast event;
      var index = loaded.notes.indexOf(miss.note);
      if (index < 0)
      {
        fail('tick ' + tick.sim + ': miss event references a note outside the driven player-note list');
      }
      var note = miss.note;
      var rawDiff = conductor.songPosition - note.noteData.time;
      var missScore = Scoring.getMissScore();
      return header("miss", tick.sim, 0.0, index, note.direction, "miss", rawDiff, Std.int(rawDiff))
        + ',"score_delta":' + Std.string(missScore)
        + ',"health_before":' + fnum(record.health)
        + ',"health_after":' + fnum(ps.health)
        + ',"combo_before":' + Std.string(record.combo)
        + ',"combo_after":' + Std.string(Highscore.tallies.combo)
        + ',"combo_max":' + Std.string(Highscore.tallies.maxCombo)
        + ',"tallies":' + talliesJson() + '}';
    }
    fail('tick ' + tick.sim + ': unexpected dispatched event ' + describeEvent(event)
      + '; the note probe only emits hit/miss/ghost_press/hold_drop');
    return "";
  }

  /** Exact §2 shared header, in the slice.rs serde field order. */
  static function header(kind:String, sim:Int, inputMs:Float, noteId:Int, direction:Int, judgement:String, rawDiff:Float,
      intDiff:Int):String
  {
    return '{"schema":"1","kind":"' + kind + '"'
      + ',"sim":' + Std.string(sim)
      + ',"input_ms":' + fnum(inputMs)
      + ',"input_time_us":0.0'
      + ',"note_id":' + Std.string(noteId)
      + ',"note_direction":' + Std.string(direction)
      + ',"judgement":"' + judgement + '"'
      + ',"raw_note_diff":' + fnum(rawDiff)
      + ',"note_diff_int":' + Std.string(intDiff);
  }

  static function talliesJson():String
  {
    return '{"sick":' + Std.string(Highscore.tallies.sick)
      + ',"good":' + Std.string(Highscore.tallies.good)
      + ',"bad":' + Std.string(Highscore.tallies.bad)
      + ',"shit":' + Std.string(Highscore.tallies.shit)
      + ',"missed":' + Std.string(Highscore.tallies.missed)
      + ',"total_notes_hit":' + Std.string(Highscore.tallies.totalNotesHit) + '}';
  }

  static function boolJson(value:Bool):String
  {
    return value ? "true" : "false";
  }

  /** Finite f64 writer: always keeps a decimal/exponent so JSON types it as f64. */
  static function fnum(value:Float):String
  {
    if (!Math.isFinite(value))
    {
      fail('refusing to serialize a non-finite f64 row field');
    }
    var text = Std.string(value);
    if (text.indexOf('.') == -1 && text.indexOf('e') == -1 && text.indexOf('E') == -1)
    {
      text += '.0';
    }
    return text;
  }
}

typedef ProbeInput =
{
  kind:String,
  direction:Int,
  input_ms:Int
};

typedef ProbeTick =
{
  sim:Int,
  music_ms:Float,
  events:Array<ProbeInput>
};

typedef ProbeStream =
{
  difficulty:String,
  tick_count:Int,
  ticks:Array<ProbeTick>
};

typedef LoadedNotes =
{
  notes:Array<NoteSprite>,
  holds:Array<SustainTrail>,
  holdNoteIds:Array<Int>,
  lastReleaseMs:Array<Int>
};

typedef NoteProbeDispatch =
{
  event:ScriptEvent,
  health:Float,
  combo:Int
};
