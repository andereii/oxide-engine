#if macro
import haxe.macro.Context;
import haxe.macro.Expr;

/**
 * ProbeSlice-style field retention for the REAL funkin.play.PlayState imported
 * unchanged into the generated game-copy tree (tools/oracle/NOTE-BEHAVIOR.md
 * section 2). Only the note-score frame path and the fields its retained
 * bodies read/write are kept; every body stays byte-identical. The transitive
 * closure is visible here: the probe fails to compile if a retained body needs
 * a dropped field.
 */
class NotePlayStateSlice
{
  static final KEEP:Array<String> = [
    // Driver/Conductor-visible state (Conductor.get_bpm reads instance/startingSong/startTimestamp).
    "instance",
    "startingSong",
    "startTimestamp",
    // Retained note-score path methods.
    "processInputQueue",
    "processNotes",
    "goodNoteHit",
    "onNoteMiss",
    "ghostNoteMiss",
    "applyScore",
    "popUpScore",
    "dispatchEvent",
    // Fields the retained bodies read or write.
    "health",
    "songScore",
    "isPracticeMode",
    "isBotPlayMode",
    "isInCutscene",
    "disableKeys",
    "inputPressQueue",
    "inputReleaseQueue",
    "playerStrumline",
    "opponentStrumline",
    "vocals",
    "playerVocalsVolume",
    "opponentVocalsVolume",
    "currentStage",
    "currentSong",
    "songEvents",
    "currentConversation",
    "comboPopUps"
  ];

  public static function keepNotePath():Array<Field>
  {
    var fields = Context.getBuildFields().filter(function(field) return KEEP.indexOf(field.name) != -1);
    // The real constructor is not retained (it pulls the gameplay closure).
    // @:nullSafety still requires an initializer on kept non-nullable fields
    // that the constructor used to assign. This adds an initializer expression
    // only; no retained method body is rewritten. The driver overwrites the
    // strumlines before any retained call. currentSong/comboPopUps are only
    // handed to no-op shells.
    for (field in fields)
    {
      switch (field.kind)
      {
        case FVar(type, null):
          switch (field.name)
          {
            case "playerStrumline" | "opponentStrumline":
              field.kind = FVar(type, macro new funkin.play.notes.Strumline());
            case "currentSong":
              field.kind = FVar(type, macro new funkin.play.song.Song());
            case "comboPopUps":
              field.kind = FVar(type, macro new funkin.play.components.PopUpStuff());
            default:
          }
        default:
      }
    }
    return fields;
  }
}
#end
