/**
 * One entry of a song's `timeChanges` array.
 *
 * Field names and optionality transcribe `funkin/data/song/SongData.hx:L157-203`:
 * `t` => timeStamp, `b` => beatTime, `n` => timeSignatureNum,
 * `d` => timeSignatureDen, `bt` => beatTuplets (unused by Conductor).
 * `beatTime` is always recomputed by `OracleConductor.mapTimeChanges`, exactly
 * like the reference implementation ignores the supplied `b` for ts > 0.
 */
class OracleTimeChange {
  /** SongData.hx:L168 (alias "t"). Milliseconds. */
  public var timeStamp:Float;

  /** SongData.hx:L175 (alias "b"). Recomputed by mapTimeChanges. */
  public var beatTime:Float;

  /** SongData.hx:L182 (alias "bpm"). Quarter notes per minute. */
  public var bpm:Float;

  /** SongData.hx:L188 (alias "n"). Defaults to 4. */
  public var timeSignatureNum:Int;

  /** SongData.hx:L194 (alias "d"). Defaults to 4. */
  public var timeSignatureDen:Int;

  public function new(timeStamp:Float, bpm:Float, timeSignatureNum:Int = 4, timeSignatureDen:Int = 4, beatTime:Float = 0.0) {
    this.timeStamp = timeStamp;
    this.bpm = bpm;
    this.timeSignatureNum = timeSignatureNum;
    this.timeSignatureDen = timeSignatureDen;
    this.beatTime = beatTime;
  }
}
