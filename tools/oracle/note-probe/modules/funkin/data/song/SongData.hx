package funkin.data.song;

/**
 * Probe shadow of the funkin.data.song.SongData MODULE. Only the data shapes
 * the retained note path reads exist. SongTimeChange is copied from
 * source/funkin/data/song/SongData.hx:L157-231 (same as the proven conductor
 * probe); SongNoteData copies the wire fields t/d/l/k (L1047-1160) and the
 * direction accessors the note path calls.
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

class SongNoteData
{
  public var time:Float;
  public var data:Int;
  public var length:Float;
  public var kind:Null<String>;
  public var params:Array<NoteParamData>;

  public function new(time:Float, data:Int, length:Float = 0, kind:String = '', ?params:Array<NoteParamData>)
  {
    this.time = time;
    this.data = data;
    this.length = length;
    this.kind = kind;
    this.params = params ?? [];
  }

  public inline function getDirection(strumlineSize:Int = 4):Int
  {
    return this.data % strumlineSize;
  }

  public function getStrumlineIndex(strumlineSize:Int = 4):Int
  {
    return Math.floor(this.data / strumlineSize);
  }

  public function getMustHitNote(strumlineSize:Int = 4):Bool
  {
    return getStrumlineIndex(strumlineSize) == 0;
  }
}

class NoteParamData
{
  public var name:String;
  public var value:Dynamic;

  public function new(name:String, value:Dynamic)
  {
    this.name = name;
    this.value = value;
  }
}

class SongEventData
{
  public var time:Float = 0.0;

  public function new() {}
}

class SongCharacterData
{
  public function new() {}
}
