package funkin.play.notes;

import funkin.data.song.SongData.SongNoteData;

/**
 * Probe shadow of funkin.play.notes.NoteSprite (data holder). Bodies for
 * strumTime and direction are copied VERBATIM from
 * source/funkin/play/notes/NoteSprite.hx:L27-110; `frames` stays null in the
 * probe so the direction setter short-circuits before playNoteAnimation
 * (engine decision, NOTE-BEHAVIOR.md section 3). The remaining fields are the
 * [V] state surface the retained PlayState/GRhythmUtil bodies read.
 */
class NoteSprite
{
  public var alive:Bool = true;
  public var holdNoteSprite:SustainTrail;

  /** Stand-in for FlxSprite.frames; the probe keeps it null. */
  public var frames:Dynamic = null;

  public var strumTime(get, set):Float;

  function get_strumTime():Float
  {
    return this.noteData?.time ?? 0.0;
  }

  function set_strumTime(value:Float):Float
  {
    if (this.noteData == null) return value;
    return this.noteData.time = value;
  }

  public var direction(default, set):NoteDirection;

  function set_direction(value:Int):Int
  {
    if (frames == null) return value;

    playNoteAnimation(value);

    this.direction = value;
    return this.direction;
  }

  function playNoteAnimation(value:Int):Void {}

  public var noteData:SongNoteData;

  public var scoreable:Bool = true;

  public var isHoldNote(get, never):Bool;

  function get_isHoldNote():Bool
  {
    return noteData.length > 0;
  }

  public var yOffset:Float = 0.0;
  public var hasBeenHit:Bool = false;
  public var lowPriority:Bool = false;
  public var hasMissed:Bool = false;
  public var tooEarly:Bool = false;
  public var mayHit:Bool = false;
  public var handledMiss:Bool = false;

  public function new() {}
}
