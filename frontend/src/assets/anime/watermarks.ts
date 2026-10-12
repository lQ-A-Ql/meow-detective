import flowerPortrait from './stickers/01-flower-portrait.webp';
import quietInvestigator from './stickers/02-quiet-investigator.webp';
import turningPortrait from './stickers/03-turning-portrait.webp';
import detectivePortrait from './stickers/04-detective-portrait.webp';
import recliningPortrait from './stickers/05-reclining-portrait.webp';
import thoughtfulPortrait from './stickers/06-thoughtful-portrait.webp';
import umbrellaPortrait from './stickers/07-umbrella-portrait.webp';
import cloudPortrait from './stickers/08-cloud-portrait.webp';
import restingCat from './stickers/09-resting-cat.webp';
import hoodedPortrait from './stickers/10-hooded-portrait.webp';
import floralPortrait from './stickers/11-floral-portrait.webp';
import headphonesPortrait from './stickers/12-headphones-portrait.webp';
import readingPortrait from './stickers/13-reading-portrait.webp';
import maskPortrait from './stickers/14-mask-portrait.webp';
import bouquetPortrait from './stickers/15-bouquet-portrait.webp';
import profilePortrait from './stickers/16-profile-portrait.webp';
import sitting from '../brand/watermark-sitting-cat.webp';
import walking from '../brand/watermark-walking-cat.webp';
import documentPaw from '../brand/watermark-document-paw.webp';

/** Artwork ownership stays in the asset bank; views use semantic motif names. */
export const watermarkArtwork = {
  'flower-portrait': flowerPortrait,
  'quiet-investigator': quietInvestigator,
  'turning-portrait': turningPortrait,
  'detective-portrait': detectivePortrait,
  'reclining-portrait': recliningPortrait,
  'thoughtful-portrait': thoughtfulPortrait,
  'umbrella-portrait': umbrellaPortrait,
  'cloud-portrait': cloudPortrait,
  'resting-cat': restingCat,
  'hooded-portrait': hoodedPortrait,
  'floral-portrait': floralPortrait,
  'headphones-portrait': headphonesPortrait,
  'reading-portrait': readingPortrait,
  'mask-portrait': maskPortrait,
  'bouquet-portrait': bouquetPortrait,
  'profile-portrait': profilePortrait,
  sitting, walking, documentPaw,
} as const;

export type WatermarkArtwork = keyof typeof watermarkArtwork;
