package funkin.modding.events;

import funkin.play.notes.NoteSprite;
import funkin.play.notes.SustainTrail;
import funkin.play.notes.NoteDirection;

/**
 * Probe shadow of the script-event data classes. The retained PlayState path
 * constructs/reads exactly these fields; each field/constructor body is copied
 * VERBATIM from source/funkin/modding/events/ScriptEvent.hx with the line
 * citation in the class doc. Unrelated event types and openfl/SongData imports
 * are dropped (closure too large for the slice).
 */
enum abstract ScriptEventType(String) from String to String
{
  // ScriptEventType.hx values used by this path.
  public var NOTE_HIT = 'NOTE_HIT';
  public var NOTE_MISS = 'NOTE_MISS';
  public var NOTE_GHOST_MISS = 'NOTE_GHOST_MISS';
  public var NOTE_HOLD_DROP = 'NOTE_HOLD_DROP';
  public var GAME_OVER = 'GAME_OVER';
}

/** ScriptEvent.hx:L20-91 (fields/cancel surface, verbatim). */
@:nullSafety
class ScriptEvent
{
  public var cancelable(default, null):Bool;
  public var type(default, null):ScriptEventType;
  public var shouldPropagate(default, null):Bool;
  public var eventCanceled(default, null):Bool;

  public function new(type:ScriptEventType, cancelable:Bool = false):Void
  {
    this.type = type;
    this.cancelable = cancelable;
    this.eventCanceled = false;
    this.shouldPropagate = true;
  }

  public function cancelEvent():Void
  {
    if (cancelable)
    {
      eventCanceled = true;
    }
  }

  public function cancel():Void
  {
    cancelEvent();
  }

  public function stopPropagation():Void
  {
    shouldPropagate = false;
  }
}

/** NoteScriptEvent.hx (ScriptEvent.hx:L93-130), verbatim fields/constructor. */
class NoteScriptEvent extends ScriptEvent
{
  public var note(default, null):NoteSprite;
  public var comboCount(default, null):Int;
  public var playSound(default, default):Bool;
  public var healthChange:Float;

  public function new(type:ScriptEventType, note:NoteSprite, healthChange:Float, comboCount:Int = 0, cancelable:Bool = false):Void
  {
    super(type, cancelable);
    this.note = note;
    this.comboCount = comboCount;
    this.playSound = true;
    this.healthChange = healthChange;
  }
}

/** HitNoteScriptEvent (ScriptEvent.hx:L133-170), verbatim fields/constructor. */
class HitNoteScriptEvent extends NoteScriptEvent
{
  public var judgement:String;
  public var score:Float;
  public var isComboBreak:Bool = false;
  public var hitDiff:Float = 0;
  public var doesNotesplash:Bool = false;

  public function new(note:NoteSprite, healthChange:Float, score:Float, judgement:String, isComboBreak:Bool, comboCount:Int = 0, hitDiff:Float = 0,
      doesNotesplash:Bool = false):Void
  {
    super(NOTE_HIT, note, healthChange, comboCount, true);
    this.score = score;
    this.judgement = judgement;
    this.isComboBreak = isComboBreak;
    this.doesNotesplash = doesNotesplash;
    this.hitDiff = hitDiff;
  }
}

/** GhostMissNoteScriptEvent (ScriptEvent.hx:L182-224), verbatim fields/constructor. */
class GhostMissNoteScriptEvent extends ScriptEvent
{
  public var dir(default, null):NoteDirection;
  public var hasPossibleNotes(default, null):Bool;
  public var healthChange(default, default):Float;
  public var scoreChange(default, default):Float;
  public var playSound(default, default):Bool;
  public var playAnim(default, default):Bool;

  public function new(dir:NoteDirection, hasPossibleNotes:Bool, healthChange:Float, scoreChange:Float):Void
  {
    super(NOTE_GHOST_MISS, true);
    this.dir = dir;
    this.hasPossibleNotes = hasPossibleNotes;
    this.healthChange = healthChange;
    this.scoreChange = scoreChange;
    this.playSound = true;
    this.playAnim = true;
  }
}

/** HoldNoteScriptEvent (ScriptEvent.hx:L232-276), verbatim fields/constructor. */
class HoldNoteScriptEvent extends NoteScriptEvent
{
  public var holdNote:SustainTrail;
  public var score:Float;
  public var isComboBreak:Bool = false;
  public var hitDiff:Float = 0;
  public var doesNotesplash:Bool = false;

  public function new(type:ScriptEventType, holdNote:SustainTrail, healthChange:Float, score:Float, isComboBreak:Bool, comboCount:Int = 0,
      cancelable:Bool = false):Void
  {
    super(type, null, healthChange, comboCount, true);
    this.holdNote = holdNote;
    this.score = score;
    this.isComboBreak = isComboBreak;
  }
}
