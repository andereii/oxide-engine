package funkin.util;

import flixel.util.FlxSignal;

/**
 * Probe stub for funkin.util.TimerUtil. The retained real Conductor touches
 * only the static `SongSequence.update` FlxSignal from here (dispatched in the
 * update() epilogue; source/funkin/util/TimerUtil.hx:L179). The stub keeps
 * exactly that shape so `Conductor.update()`'s retained code compiles and the
 * real runtime path is unchanged; the sequence playback machinery is
 * unrelated to the retained update path and is dropped.
 */
@:nullSafety
class SongSequence
{
  static final update:FlxSignal = new FlxSignal();
}
