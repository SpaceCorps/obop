import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Banner } from "./Banner.tsx";
import { Button } from "./Button.tsx";
import { Field } from "./Field.tsx";
import { Row } from "./Row.tsx";
import { Screen } from "./Screen.tsx";
import { Text } from "./Text.tsx";
import { Padded, PhoneFrames } from "../stories-support.tsx";

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
      <Screen
        title="Recording"
        scroll={false}
        footer={<Button label="Stop" variant="danger" onPress={noop} />}
      >
        <Text variant="display">00:14:32</Text>
        <Text tone="muted">3.1 km so far (this phone&apos;s count; the server decides)</Text>
        <Banner
          tone="info"
          title="Keep this app running"
          message="Do not force-stop it while you drive."
        />
      </Screen>
    </PhoneFrames>
  ),
};
