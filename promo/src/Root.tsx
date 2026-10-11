import React from "react";
import { Composition, Still } from "remotion";
import { CHAPTERS, CHAPTER_SECONDS, Chapter, ChapterId, Hero, LAUNCH_SECONDS, Launch } from "./scenes/Launch";
import { HomeStill } from "./scenes/HomeStill";
import { NOTIFICATIONS_SECONDS, NOTIFICATIONS_SIZE, Notifications } from "./scenes/Notifications";
import {
  HOME_SECONDS,
  Home,
  LANGUAGES_SECONDS,
  LAYOUTS_SECONDS,
  Languages,
  Layouts,
  POPUP_STAGE,
  THEMES_SECONDS,
  Themes,
  USAGE_SECONDS,
  Usage,
} from "./scenes/PopupScenes";

export const FPS = 60;

const clip = (id: string, component: React.FC<any>, seconds: number, size = POPUP_STAGE, props = {}) => (
  <Composition
    id={id}
    component={component}
    durationInFrames={Math.round(seconds * FPS)}
    fps={FPS}
    width={size.width}
    height={size.height}
    defaultProps={props}
  />
);

export const Root: React.FC = () => (
  <>
    <Still id="HomeStill" component={HomeStill} width={520} height={1180} />
    {clip("Notifications", Notifications, NOTIFICATIONS_SECONDS, NOTIFICATIONS_SIZE, { dark: true })}
    {clip("NotificationsLight", Notifications, NOTIFICATIONS_SECONDS, NOTIFICATIONS_SIZE, { dark: false })}
    {clip("Home", Home, HOME_SECONDS, { width: POPUP_STAGE.width, height: 1160 })}
    {clip("Layouts", Layouts, LAYOUTS_SECONDS, { width: POPUP_STAGE.width, height: 420 })}
    {clip("Themes", Themes, THEMES_SECONDS, { width: POPUP_STAGE.width, height: 930 })}
    {clip("Languages", Languages, LANGUAGES_SECONDS, { width: POPUP_STAGE.width, height: 840 })}
    {clip("Usage", Usage, USAGE_SECONDS, { width: POPUP_STAGE.width, height: 800 })}
    {clip("Launch", Launch, LAUNCH_SECONDS, { width: 1920, height: 1080 })}
    {/* Release notes media, 16:9. */}
    <Still id="Hero" component={Hero} width={1920} height={1080} />
    {(Object.keys(CHAPTERS) as ChapterId[]).map((id) =>
      clip(`Chapter-${id}`, Chapter, CHAPTER_SECONDS, { width: 1920, height: 1080 }, { id }),
    )}
  </>
);
