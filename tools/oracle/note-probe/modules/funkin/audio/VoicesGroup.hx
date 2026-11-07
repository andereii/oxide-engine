package funkin.audio;

/**
 * Probe shadow of funkin.audio.VoicesGroup: only the mute/legacy fields the
 * retained hit/miss bodies read. The probe has no audio output; vocals stays
 * null so these paths are not evaluated (NOTE-BEHAVIOR.md section 3).
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
