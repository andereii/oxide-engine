package funkin.play.character;

import funkin.play.notes.NoteDirection;

/**
 * Probe shadow of the character shell: only the fields the retained
 * processNotes/onNoteMiss bodies read (isSinging/holdTimer/tempVocals) and the
 * miss animation surface. [S] shell, not a character simulation.
 *
 * `singing` defaults false, so the round-1 path (currentStage null) and any
 * caller that does not configure the shell keep the old `isSinging() == false`
 * behavior. Round 2 sets it true on the dad shell only to execute the retained
 * opponent holdTimer-reset branch (NOTE-BEHAVIOR.md section 10).
 */
class BaseCharacter
{
  public var holdTimer:Float = 0;
  public var tempVocals:Bool = false;
  /** [S] Probe switch for the retained isSinging() read; no animation state. */
  public var singing:Bool = false;

  public function new() {}

  public function isSinging():Bool
  {
    return singing;
  }

  public function playSingAnimation(direction:NoteDirection, miss:Bool = false):Void {}

  public function getDeathPreTransitionDelay():Float
  {
    return 0.0;
  }
}
