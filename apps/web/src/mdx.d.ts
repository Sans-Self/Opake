declare module "*.mdx" {
  import type { ComponentType } from "react";
  // eslint-disable-next-line functional/prefer-immutable-types -- module declaration
  const Component: ComponentType;
  export default Component;
}
