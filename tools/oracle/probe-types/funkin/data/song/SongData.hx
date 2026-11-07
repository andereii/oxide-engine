package funkin.data.song;

/**
 * Probe stub for the funkin.data.song.SongData MODULE (probe-types classpath
 * shadows the real module so the huge chart-editor/registry closure is not
 * compiled). Only the `SongTimeChange` shapes the retained real
 * Conductor/SongDataUtils code touches are provided; field default metadata
 * and the constructor are copied verbatim from the real class
 * (source/funkin/data/song/SongData.hx L157-231) with its line citation. The
 * probe never serializes this type, so unused json-metadata surfaces are
 * removed.
 */
class SongTimeChange
{
  @:alias("t")
  public var timeStamp:Float;

  @:optional @:alias("b")
  public var beatTime:Float;

  @:alias("bpm")
  public var bpm:Float;

  @:default(4) @:optional @:alias("n")
  public var timeSignatureNum:Int;

  @:default(4) @:optional @:alias("d")
  public var timeSignatureDen:Int;

  static final DEFAULT_BEAT_TIME:Null<Float> = null;

  public function new(timeStamp:Float, bpm:Float, timeSignatureNum:Int = 4, timeSignatureDen:Int = 4, ?beatTime:Float, ?beatTuplets:Array<Int>)
  {
    this.timeStamp = timeStamp;
    this.bpm = bpm;
    this.timeSignatureNum = timeSignatureNum;
    this.timeSignatureDen = timeSignatureDen;
    this.beatTime = beatTime == null ? DEFAULT_BEAT_TIME : beatTime;
  }
}
