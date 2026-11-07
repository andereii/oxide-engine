package funkin.play.notes;

import funkin.Conductor;
import funkin.util.Constants;
import funkin.modding.events.ScriptEvent;

/**
 * Probe shadow of funkin.play.notes.Strumline (NOTE-BEHAVIOR.md section 3).
 *
 * KEY_COUNT [V] (Strumline.hx:L47). getNotesMayHit [V] (L360-366).
 * pressKey/releaseKey [V] keptKeys semantics (L840-865, minus the audio/animation
 * surfaces), hitNote [S] citing the real body's observable state at L952-990:
 * hasBeenHit, kill (alive=false) for removeNote=true, hold hitNote/missedNote
 * and the sustainLength min() adjustment. playPress/playConfirm/playStatic/
 * playNoteSplash/playNoteHoldCover are animation surfaces with no slice
 * observables ([D]: excluded from the trace). mayGhostTap is [S] (the shipped
 * release defines NO_FEATURE_GHOST_TAPPING, so processInputQueue does not call
 * it; the shell still documents the real condition).
 */
@:nullSafety
class Strumline
{
  public static final KEY_COUNT:Int = 4;

  public var notes:NoteGroup<NoteSprite> = new NoteGroup<NoteSprite>();
  public var holdNotes:NoteGroup<SustainTrail> = new NoteGroup<SustainTrail>();
  public var noteVibrations:NoteVibrationsHandler = new NoteVibrationsHandler();
  public var isPlayer:Bool = true;

  var heldKeys:Array<Array<Int>> = [[], [], [], []];

  public function new() {}

  /** Strumline.hx:L360-366, verbatim. */
  public function getNotesMayHit():Array<NoteSprite>
  {
    return notes.members.filter(function(note:NoteSprite)
    {
      return note != null && note.alive && !note.hasBeenHit && note.mayHit;
    });
  }

  /** Strumline.hx:L335-355 shell: no notes/holds may be tapped. */
  public function mayGhostTap():Bool
  {
    if (getNotesMayHit().length > 0) return false;
    for (holdNote in holdNotes.members)
    {
      if (holdNote != null && holdNote.alive && (holdNote.hitNote || holdNote.missedNote)) return false;
    }
    return true;
  }

  /** Strumline.hx:L840-843, verbatim key bookkeeping. */
  public function pressKey(dir:NoteDirection, keyCode:Int):Void
  {
    heldKeys[dir].push(keyCode);
  }

  /** Strumline.hx:L851-860, verbatim key bookkeeping. */
  public function releaseKey(dir:NoteDirection, ?keyCode:Int):Void
  {
    if (keyCode == null)
    {
      heldKeys[dir] = [];
    }
    else
    {
      heldKeys[dir].remove(keyCode);
    }
  }

  public function isKeyHeld(dir:NoteDirection):Bool
  {
    return heldKeys[dir].length > 0;
  }

  public function pressKeys(dir:NoteDirection):Array<Int>
  {
    return heldKeys[dir];
  }

  /** Animation surface (no slice observable). */
  public function playStatic(direction:NoteDirection):Void {}

  /** Animation surface (no slice observable). */
  public function playPress(direction:NoteDirection):Void {}

  /** Animation surface (no slice observable). */
  public function playConfirm(direction:NoteDirection):Void {}

  /**
   * [S] Strumline.hx:L952-990: preserve hasBeenHit, the removeNote kill, and
   * the hold-sprite state/sustainLength adjustment the retained processNotes
   * drop path reads. The alpha/desaturate/cover/sound surfaces are excluded.
   */
  public function hitNote(note:NoteSprite, removeNote:Bool = true):Void
  {
    playConfirm(note.direction);
    note.hasBeenHit = true;

    if (removeNote)
    {
      note.alive = false;
    }

    if (note.holdNoteSprite != null)
    {
      note.holdNoteSprite.hitNote = true;
      note.holdNoteSprite.missedNote = false;

      note.holdNoteSprite.sustainLength = Math.min(note.holdNoteSprite.fullSustainLength,
        (note.holdNoteSprite.strumTime + note.holdNoteSprite.fullSustainLength) - Conductor.instance.songPosition);
    }
  }

  /** Animation surface (no slice observable). */
  public function playNoteSplash(direction:NoteDirection):Void {}

  /** Animation surface (no slice observable). */
  public function playNoteHoldCover(holdNote:SustainTrail):Void {}
}

class NoteGroup<T>
{
  public var members:Array<T> = [];

  public function new() {}
}
