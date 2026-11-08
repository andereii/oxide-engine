package funkin.audio;

/**
 * Probe shadow of funkin.audio.VoicesGroup [S]: only the mute/legacy fields the
 * retained hit/miss bodies read; the probe has no audio output.
 *
 * Round 1 keeps `vocals` null, so those paths are not evaluated. Round 2 sets a
 * configured shell (`legacyVoiceSystem=true`, `legacyVoiceUsesPlayer=false`) so
 * the retained opponent hit branch exercises the real `opponentVolume`
 * assignment (NOTE-BEHAVIOR.md section 10).
 */
@:nullSafety
class VoicesGroup
{
  public var legacyVoiceSystem:Bool = false;
  public var legacyVoiceUsesPlayer:Bool = false;
  public var playerVolume:Float = 1.0;
  public var opponentVolume:Float = 1.0;

  public function new() {}

  public function pause():Void {}

  public function stop():Void {}
}
