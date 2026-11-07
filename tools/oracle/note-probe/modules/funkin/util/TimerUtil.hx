package funkin.util;

import flixel.util.FlxSignal;

/**
 * Probe shadow of funkin.util.TimerUtil.SongSequence: the retained Conductor
 * update() dispatches the static update signal (Conductor.hx:L516); the
 * sequence playback machinery is unrelated and dropped.
 */
@:nullSafety
class SongSequence
{
  static final update:FlxSignal = new FlxSignal();
}
