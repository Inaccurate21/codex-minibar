import React from "react";
import { AbsoluteFill } from "remotion";
import { Desktop, Fonts } from "../ui/Desktop";
import { Footer, HomePage, PopupFrame } from "../ui/Popup";
import { fluent } from "../ui/palette";
import { STRINGS, sections } from "../data";

export const HomeStill: React.FC = () => {
  const p = fluent(true);
  return (
    <AbsoluteFill>
      <Fonts />
      <Desktop />
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>
        <PopupFrame p={p} height={1100} footer={<Footer p={p} active={0} />}>
          <HomePage p={p} s={STRINGS.en.s} sections={sections()} />
        </PopupFrame>
      </AbsoluteFill>
    </AbsoluteFill>
  );
};
