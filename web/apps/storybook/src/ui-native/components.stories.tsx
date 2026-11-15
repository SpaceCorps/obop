import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Banner, Button, Field, Row, Screen, Text } from "@obop/ui-native";
import { Padded, PhoneFrames } from "@obop/ui-native/stories-support";

const meta = {
  title: "Native/Components",
  parameters: { layout: "fullscreen" },
} satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;

const noop = () => {};

export const Buttons: Story = {
  render: () => (
    <PhoneFrames height={560}>
      <Padded>
        <Button label="Primary" onPress={noop} />
        <Button label="Secondary" variant="secondary" onPress={noop} />
        <Button label="Discard trip" variant="danger" onPress={noop} />
        <Button label="Back" variant="ghost" onPress={noop} />
        <Button label="Disabled" disabled onPress={noop} />
        <Button label="Uploading" busy onPress={noop} />
        <Button label="Start" size="big" onPress={noop} />
      </Padded>
    </PhoneFrames>
  ),
};

export const Rows: Story = {
  render: () => (
    <PhoneFrames height={420}>
      <Row title="12 Oct, 08:14" subtitle="Ended" value="12.4 km" onPress={noop} />
      <Row title="11 Oct, 17:02" subtitle="In review" value="8.1 km" onPress={noop} />
      <Row title="Server" value="https://obop.example" />
      <Row title="Storage" value="Encrypted on this phone (SQLCipher 4.17.0 community)" />
      <Row title="Pixel 9" subtitle="Android 16, this phone" />
    </PhoneFrames>
  ),
};

function FieldDemo() {
  const [reason, setReason] = useState("");
  return (
    <Padded>
      <Field label="Reason" value={reason} onChangeText={setReason} placeholder="Why discard?" />
      <Field
        label="Server address"
        value="https://obop.example"
        onChangeText={noop}
        helper="The address of your obop instance."
      />
      <Field label="Device name" value="" onChangeText={noop} error="Give the device a name." />
      <Field label="Account" value="driver@example.com" onChangeText={noop} editable={false} />
    </Padded>
  );
}

export const Fields: Story = {
  render: () => (
    <PhoneFrames height={560}>
      <FieldDemo />
    </PhoneFrames>
  ),
};

export const Banners: Story = {
  render: () => (
    <PhoneFrames height={620}>
      <Padded>
        <Banner tone="info" title="Recording" message="Your route is saved on this phone." />
        <Banner tone="success" title="Uploaded" message="All 412 points are on the server." />
        <Banner
          tone="warning"
          title="A trip is still open"
          message="Started 3 h ago. Stop it or discard it."
          action={{ label: "Open trip", onPress: noop }}
        />
        <Banner
          tone="danger"
          title="Could not end the trip"
          message="The server refused it: no distance."
        />
      </Padded>
    </PhoneFrames>
  ),
};

export const HomeScreen: Story = {
  render: () => (
    <PhoneFrames height={700}>
      <Screen title="Mileage">
        <Banner tone="warning" title="A trip is still open" message="Started 3 h ago." />
        <Button label="Start" size="big" onPress={noop} hint="Starts recording a trip" />
        <Text tone="muted">Last trip: 12.4 km, ended 08:41</Text>
        <Row title="Trips" onPress={noop} />
        <Row title="Account" onPress={noop} />
      </Screen>
    </PhoneFrames>
  ),
};

export const RecordingScreen: Story = {
  render: () => (
    <PhoneFrames height={700}>
      <Screen title="Recording" footer={<Button label="Stop" variant="danger" onPress={noop} />}>
        <Banner
          tone="danger"
          title="Location updates are not running"
          message="The trip is open but the phone is not recording it. Restart recording."
          action={{ label: "Restart recording", onPress: noop }}
        />
        <Text variant="display">00:14:32</Text>
        <Text>
          3.1 km so far. This is the phone&apos;s own count; the server decides the distance.
        </Text>
        <Text tone="muted">212 points saved on this phone</Text>
        <Banner
          tone="info"
          title="Keep this app running"
          message="Do not force-stop it while you drive."
        />
      </Screen>
    </PhoneFrames>
  ),
};

function PurposeDemo() {
  const [purpose, setPurpose] = useState("business");
  return (
    <Padded>
      <Text variant="label">Purpose of the trip</Text>
      {["business", "private", "commute"].map((p) => (
        <Button
          key={p}
          label={p.charAt(0).toUpperCase() + p.slice(1)}
          variant={p === purpose ? "primary" : "secondary"}
          selected={p === purpose}
          onPress={() => setPurpose(p)}
        />
      ))}
    </Padded>
  );
}

/** One choice of a group: the chosen one carries a check mark as well as the fill, and a screen reader hears a radio button. */
export const PurposePicker: Story = {
  render: () => (
    <PhoneFrames height={420}>
      <PurposeDemo />
    </PhoneFrames>
  ),
};
