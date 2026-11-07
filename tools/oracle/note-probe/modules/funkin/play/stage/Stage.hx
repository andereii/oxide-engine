package funkin.play.stage;

import funkin.modding.events.ScriptEvent;
import funkin.play.character.BaseCharacter;

/**
 * Probe shadow of funkin.play.stage.Stage. The retained miss/hold paths only
 * query the character shells; the opponent side is excluded and currentStage
 * stays null in the probe (NOTE-BEHAVIOR.md section 3). dispatchToCharacters is
 * a no-op surface called by the retained PlayState.dispatchEvent.
 */
class Stage
{
  public function new() {}

  public function getBoyfriend():BaseCharacter
  {
    return null;
  }

  public function getOpponent():BaseCharacter
  {
    return null;
  }

  public function getDad():BaseCharacter
  {
    return null;
  }

  public function dispatchToCharacters(event:ScriptEvent):Void {}

  public function resetStage():Void {}
}
