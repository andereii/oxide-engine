package funkin.play.character;

import funkin.play.notes.NoteDirection;

/**
 * Probe shadow of the character shell: only the fields the retained
 * processNotes/onNoteMiss bodies read (isSinging/holdTimer/tempVocals) and the
 * miss animation surface.
 */
class BaseCharacter
{
  public var holdTimer:Float = 0;
  public var tempVocals:Bool = false;

  public function new() {}

  public function isSinging():Bool
  {
    return false;
  }

  public function playSingAnimation(direction:NoteDirection, miss:Bool = false):Void {}

  public function getDeathPreTransitionDelay():Float
  {
    return 0.0;
  }
}
