import { helper } from "./helper";
export class Service {
  run() {
    helper();
  }
}
export function main() {
  new Service().run();
}
const local = () => main();
