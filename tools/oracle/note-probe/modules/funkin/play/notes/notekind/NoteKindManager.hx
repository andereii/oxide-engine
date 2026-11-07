package funkin.play.notes.notekind;

import funkin.modding.events.ScriptEvent;

/** Probe shadow: no note-kind scripts exist; dispatch is a no-op. */
class NoteKindManager
{
  public static function callEvent(event:ScriptEvent):Void {}
}
