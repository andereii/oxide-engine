package flixel;

// Import shim. The sliced real FlxMath still imports FlxSprite; this stub
// (last classpath) stops the graphics closure. No sprite operation is called
// by the retained note path.
extern class FlxSprite {}
