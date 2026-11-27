import { View } from "react-native";

/**
 * The few symbols of the app, drawn with views: the round buttons of the header, the switcher at
 * the bottom and the small round buttons of a recording (Pause, Stop, Restart). No font of icons and no SVG, so no package to add.
 */
export type IconName =
  | "back"
  | "close"
  | "sort"
  | "search"
  | "check"
  | "plus"
  | "trash"
  | "person"
  | "clock"
  | "record"
  | "pause"
  | "stop"
  | "restart"
  | "forward";

const STROKE = 2.5;

/** A bar of the stroke's width, centred in the icon and turned by `deg`. */
function Bar({ length, color, deg }: { length: number; color: string; deg: number }) {
  return (
    <View
      style={{
        position: "absolute",
        width: length,
        height: STROKE,
        borderRadius: STROKE / 2,
        backgroundColor: color,
        transform: [{ rotate: `${deg}deg` }],
      }}
    />
  );
}

export function Icon({ name, color, size = 22 }: { name: IconName; color: string; size?: number }) {
  const unit = size / 22;
  const box = {
    width: size,
    height: size,
    alignItems: "center",
    justifyContent: "center",
  } as const;
  switch (name) {
    case "back":
      return (
        <View style={box}>
          <View
            style={{
              width: 11 * unit,
              height: 11 * unit,
              marginLeft: 4 * unit,
              borderLeftWidth: STROKE,
              borderBottomWidth: STROKE,
              borderColor: color,
              transform: [{ rotate: "45deg" }],
            }}
          />
        </View>
      );
    case "forward":
      // The arrow of a row that opens its page, as in iOS: Back turned the other way.
      return (
        <View style={box}>
          <View
            style={{
              width: 11 * unit,
              height: 11 * unit,
              marginRight: 4 * unit,
              borderRightWidth: STROKE,
              borderTopWidth: STROKE,
              borderColor: color,
              transform: [{ rotate: "45deg" }],
            }}
          />
        </View>
      );
    case "close":
      return (
        <View style={box}>
          <Bar length={18 * unit} color={color} deg={45} />
          <Bar length={18 * unit} color={color} deg={-45} />
        </View>
      );
    case "sort":
      // Three lines, each shorter than the one above, as the sort of Telegram's contacts.
      return (
        <View style={[box, { gap: 4 * unit }]}>
          {[18, 13, 8].map((length) => (
            <View
              key={length}
              style={{
                width: length * unit,
                height: 2,
                borderRadius: 1,
                backgroundColor: color,
              }}
            />
          ))}
        </View>
      );
    case "search":
      return (
        <View style={box}>
          <View
            style={{
              position: "absolute",
              top: 2 * unit,
              left: 2 * unit,
              width: 13 * unit,
              height: 13 * unit,
              borderRadius: 7 * unit,
              borderWidth: 2,
              borderColor: color,
            }}
          />
          <View
            style={{
              position: "absolute",
              // From the ring's edge at the bottom right, outwards.
              top: 14.6 * unit,
              left: 12.1 * unit,
              width: 7 * unit,
              height: 2,
              borderRadius: 1,
              backgroundColor: color,
              transform: [{ rotate: "45deg" }],
            }}
          />
        </View>
      );
    case "check":
      return (
        <View style={box}>
          <View
            style={{
              width: 8 * unit,
              height: 15 * unit,
              marginTop: -4 * unit,
              borderRightWidth: STROKE,
              borderBottomWidth: STROKE,
              borderColor: color,
              transform: [{ rotate: "45deg" }],
            }}
          />
        </View>
      );
    case "plus":
      return (
        <View style={box}>
          <Bar length={18 * unit} color={color} deg={0} />
          <Bar length={18 * unit} color={color} deg={90} />
        </View>
      );
    case "trash":
      return (
        <View style={[box, { justifyContent: "flex-start", paddingTop: 1 * unit }]}>
          <View
            style={{
              width: 6 * unit,
              height: 3 * unit,
              borderWidth: 2,
              borderBottomWidth: 0,
              borderColor: color,
              borderTopLeftRadius: 1,
              borderTopRightRadius: 1,
            }}
          />
          <View
            style={{ width: 17 * unit, height: STROKE, borderRadius: 1, backgroundColor: color }}
          />
          <View
            style={{
              width: 12 * unit,
              height: 13 * unit,
              marginTop: 1 * unit,
              borderWidth: STROKE,
              borderTopWidth: 0,
              borderColor: color,
              borderBottomLeftRadius: 3 * unit,
              borderBottomRightRadius: 3 * unit,
            }}
          />
        </View>
      );
    case "person":
      return (
        <View style={[box, { justifyContent: "space-between", paddingVertical: 1 * unit }]}>
          <View
            style={{
              width: 9 * unit,
              height: 9 * unit,
              borderRadius: 5 * unit,
              borderWidth: 2,
              borderColor: color,
            }}
          />
          <View
            style={{
              width: 17 * unit,
              height: 8 * unit,
              borderWidth: 2,
              borderBottomWidth: 0,
              borderColor: color,
              borderTopLeftRadius: 9 * unit,
              borderTopRightRadius: 9 * unit,
            }}
          />
        </View>
      );
    case "clock":
      return (
        <View style={box}>
          <View
            style={{
              position: "absolute",
              width: 20 * unit,
              height: 20 * unit,
              borderRadius: 10 * unit,
              borderWidth: 2,
              borderColor: color,
            }}
          />
          <View
            style={{
              position: "absolute",
              width: 2,
              height: 6 * unit,
              top: 5 * unit,
              backgroundColor: color,
            }}
          />
          <View
            style={{
              position: "absolute",
              width: 5 * unit,
              height: 2,
              top: 10 * unit,
              left: 11 * unit,
              backgroundColor: color,
            }}
          />
        </View>
      );
    case "record":
      return (
        <View style={box}>
          <View
            style={{
              position: "absolute",
              width: 20 * unit,
              height: 20 * unit,
              borderRadius: 10 * unit,
              borderWidth: 2,
              borderColor: color,
            }}
          />
          <View
            style={{
              width: 9 * unit,
              height: 9 * unit,
              borderRadius: 5 * unit,
              backgroundColor: color,
            }}
          />
        </View>
      );
    case "pause":
      // Two upright bars.
      return (
        <View style={[box, { flexDirection: "row", gap: 4 * unit }]}>
          {[0, 1].map((bar) => (
            <View
              key={bar}
              style={{
                width: 3.5 * unit,
                height: 13 * unit,
                borderRadius: 1,
                backgroundColor: color,
              }}
            />
          ))}
        </View>
      );
    case "stop":
      // A square.
      return (
        <View style={box}>
          <View
            style={{ width: 11 * unit, height: 11 * unit, borderRadius: 2, backgroundColor: color }}
          />
        </View>
      );
    case "restart":
      // A ring open at its upper right, with the head of an arrow at its top, going round.
      return (
        <View style={box}>
          <View
            style={{
              position: "absolute",
              width: 16 * unit,
              height: 16 * unit,
              borderRadius: 8 * unit,
              borderWidth: STROKE,
              borderColor: color,
              borderTopColor: "transparent",
              transform: [{ rotate: "45deg" }],
            }}
          />
          <View
            style={{
              position: "absolute",
              left: 10 * unit,
              top: 0.5 * unit,
              width: 0,
              height: 0,
              borderLeftWidth: 6 * unit,
              borderTopWidth: 4 * unit,
              borderBottomWidth: 4 * unit,
              borderLeftColor: color,
              borderTopColor: "transparent",
              borderBottomColor: "transparent",
            }}
          />
        </View>
      );
  }
}
