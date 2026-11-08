package funkin.play.stage;

import funkin.modding.events.ScriptEvent;
import funkin.play.character.BaseCharacter;

/**
 * Probe shadow of funkin.play.stage.Stage. The retained miss/hold paths only
 * query the character shells; dispatchToCharacters is a no-op surface called
 * by the retained PlayState.dispatchEvent.
 *
 * Round 1 keeps currentStage null, so these getters are not called.
 * Round 2 sets currentStage non-null so the retained opponent hold loop reads
 * getDad()/getOpponent(); the driver assigns [S] BaseCharacter shells whose
 * isSinging/holdTimer/playSingAnimation surface is enough for that loop
 * (NOTE-BEHAVIOR.md section 10). Defaults stay null, preserving the round-1
 * path.
 */
class Stage
{
  /** [S] Round-2 character shells; null by default (round 1). */
  public var dad:BaseCharacter = null;
  public var opponent:BaseCharacter = null;

  public function new() {}

  public function getBoyfriend():BaseCharacter
  {
    return null;
  }

  public function getOpponent():BaseCharacter
  {
    return opponent;
  }

  public function getDad():BaseCharacter
  {
    return dad;
  }

  public function dispatchToCharacters(event:ScriptEvent):Void {}

  public function resetStage():Void {}
}
