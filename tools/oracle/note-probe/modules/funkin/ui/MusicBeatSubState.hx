package funkin.ui;

import flixel.FlxSubState;
import funkin.input.Controls;
import funkin.modding.events.ScriptEvent;
import NoteProbeRuntime;

/**
 * Probe shadow of funkin.ui.MusicBeatSubState. Only the surface the retained
 * PlayState module needs exists: the dispatchEvent override target and the
 * `controls` getter read by onNoteMiss/ghostNoteMiss. dispatchEvent is the
 * instrumentation point (tools/oracle/NOTE-BEHAVIOR.md section 6): it pends
 * the event and leaves eventCanceled false (no scripts on the probe).
 */
@:nullSafety
class MusicBeatSubState extends FlxSubState
{
  static var defaultControls:Controls = new Controls();

  public var controls(get, never):Controls;

  inline function get_controls():Controls
  {
    return defaultControls;
  }

  public function new()
  {
    super();
  }

  public function dispatchEvent(event:ScriptEvent):Void
  {
    NoteProbeRuntime.onDispatch(event);
  }
}
