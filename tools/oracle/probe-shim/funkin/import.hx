package;

/**
 * Probe shim for the RETAINED MODULE COPY TREE (generated under target/ by
 * tools/oracle/native_conductor_probe.sh): modules compiled from the copy
 * tree do not see the game's real funkin/import.hx (which pulls funkin.Assets,
 * Paths, Tracker, thx.*) — none of those are exercised by the retained real
 * Conductor. This shim declares exactly the imports the retained real
 * modules need: flixel.FlxG (the probe's simulated playback object),
 * StringTools core (trim), FloatTools.clamp and funkin.util.AnsiUtil
 * (bg_purple/bold). NOT a copy of any original file.
 */
#if !macro
import flixel.FlxG;

using Lambda;
using StringTools;
using funkin.util.tools.FloatTools;
using funkin.util.AnsiUtil;
#end
