package io.github.vaspyyy.lumen.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/** lumen's few line icons, drawn on a 24 grid with round ends. */
object Icons {
    private fun icon(name: String, vararg paths: String, stroke: Float = 1.8f): ImageVector {
        val b = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        for (d in paths) {
            b.addPath(
                pathData = PathParser().parsePathString(d).toNodes(),
                stroke = SolidColor(Color.White),
                strokeLineWidth = stroke,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
        }
        return b.build()
    }

    val close = icon("close", "M7 7 L17 17 M17 7 L7 17")
    val plus = icon("plus", "M12 5.5 V18.5 M5.5 12 H18.5")
    val back = icon("back", "M14.5 5.5 L8 12 L14.5 18.5")
    val toBottom = icon("toBottom", "M12 5 V18 M6.5 12.5 L12 18 L17.5 12.5")
    val up = icon("up", "M12 18 V6.5 M7 11.5 L12 6.5 L17 11.5", stroke = 2f)
    val down = icon("down", "M12 6 V17.5 M7 12.5 L12 17.5 L17 12.5", stroke = 2f)
    val left = icon("left", "M18 12 H6.5 M11.5 7 L6.5 12 L11.5 17", stroke = 2f)
    val right = icon("right", "M6 12 H17.5 M12.5 7 L17.5 12 L12.5 17", stroke = 2f)
    val torch = icon("torch", "M8 3 H16 V7 L14.5 10 V20.5 H9.5 V10 L8 7 Z", "M12 13.5 V16")
    val paste = icon(
        "paste",
        "M9 3.5 H15 V6.5 H9 Z",
        "M7.5 5 H6.5 A1.5 1.5 0 0 0 5 6.5 V19.5 A1.5 1.5 0 0 0 6.5 21 H17.5 A1.5 1.5 0 0 0 19 19.5 V6.5 A1.5 1.5 0 0 0 17.5 5 H16.5",
    )
    val copy = icon(
        "copy",
        "M9 9 H19 A1 1 0 0 1 20 10 V19 A1 1 0 0 1 19 20 H10 A1 1 0 0 1 9 19 Z",
        "M15 6 V5 A1 1 0 0 0 14 4 H5 A1 1 0 0 0 4 5 V14 A1 1 0 0 0 5 15 H6",
    )
    val monitor = icon("monitor", "M4 5 H20 A1 1 0 0 1 21 6 V15 A1 1 0 0 1 20 16 H4 A1 1 0 0 1 3 15 V6 A1 1 0 0 1 4 5 Z", "M8.5 20 H15.5 M12 16 V20")
    val trash = icon("trash", "M4.5 7 H19.5 M10 11 V16.5 M14 11 V16.5 M6.5 7 L7.4 19.2 A1.5 1.5 0 0 0 8.9 20.5 H15.1 A1.5 1.5 0 0 0 16.6 19.2 L17.5 7 M9.5 7 V4.5 H14.5 V7")
    val check = icon("check", "M5.5 12.5 L10 17 L18.5 7.5", stroke = 2.2f)
    val folder = icon("folder", "M3.5 7 A1.5 1.5 0 0 1 5 5.5 H9.5 L11.5 7.5 H19 A1.5 1.5 0 0 1 20.5 9 V17.5 A1.5 1.5 0 0 1 19 19 H5 A1.5 1.5 0 0 1 3.5 17.5 Z")
    val prompt = icon("prompt", "M5 7 L10 12 L5 17 M12.5 17 H19")
    val keyboard = icon("keyboard", "M4 6.5 H20 A1 1 0 0 1 21 7.5 V16.5 A1 1 0 0 1 20 17.5 H4 A1 1 0 0 1 3 16.5 V7.5 A1 1 0 0 1 4 6.5 Z", "M7 10 H7.01 M10.5 10 H10.51 M14 10 H14.01 M17 10 H17.01 M8 14 H16")
}
