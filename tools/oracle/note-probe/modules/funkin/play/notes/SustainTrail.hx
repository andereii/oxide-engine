package funkin.play.notes;

import funkin.data.song.SongData.SongNoteData;

/**
 * Probe shadow of funkin.play.notes.SustainTrail (data holder). Field
 * names/defaults copied from source/funkin/play/notes/SustainTrail.hx:L41-81;
 * the render/graphics closure is dropped (out of scope).
 */
class SustainTrail
{
  public var alive:Bool = true;
  public var strumTime:Float = 0;
  public var noteDirection:NoteDirection = 0;
  public var sustainLength:Float = 0;
  public var fullSustainLength:Float = 0;
  public var noteData:Null<SongNoteData>;
  public var scoreable:Bool = true;
  public var hitNote:Bool = false;
  public var missedNote:Bool = false;
  public var handledMiss:Bool = false;

  /**
   * Driver bookkeeping for the §6.1 hold-row regime. Not shipped fields.
   * probeHitSim is the tick whose processInputQueue set hitNote.
   * probeDropFlagSim is the frame-end tick that set missedNote; the hold_drop
   * row is emitted on the following tick.
   */
  public var probeHitSim:Int = -1;
  /** -1 means this hold has not been drop-flagged. */
  public var probeDropFlagSim:Int = -1;
  /** sustainLength observed when the mirror flagged the drop. Untouched by L632. */
  public var probeDropSnapshot:Float = 0;
  /** sustainLength the accrual branch observed this tick. */
  public var probeAccrualSnapshot:Float = 0;
  public var probeDropEmitted:Bool = false;
  public var probeEndSim:Int = -1;
  public var probeCompleted:Bool = false;

  public function new() {}
}
