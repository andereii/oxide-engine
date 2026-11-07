package;

/**
 * Probe shim for the generated game-copy tree (tools/oracle/note_probe.sh).
 * The real funkin/import.hx pulls Assets, Tracker and the full util chain; the
 * retained note path needs only Constants/FlxG/Paths/Preferences and the
 * FloatTools/AnsiUtil extensions. NOT a copy of any original file.
 */
#if !macro
import funkin.util.Constants;
import flixel.FlxG;
import funkin.Paths;
import funkin.Preferences;

import funkin.modding.events.ScriptEvent;
import funkin.play.scoring.Scoring;

using Lambda;
using StringTools;
using funkin.util.tools.FloatTools;
using funkin.util.tools.Int64Tools;
using funkin.util.AnsiUtil;
#end
