// The emergency guides the watch's survival app can carry, each a short text read on the watch without a signal.
//
// Each guide keeps its slot for good, since the slot names its file on the watch; a new guide takes the next free
// slot, and the watch has room for slots 0 to 23. The texts use plain ASCII, so every character is in the watch's
// font, and follow common first aid, outdoor and protest safety training; they are reminders, not a replacement for a
// course.

// The topics the watch lists the guides under, in its order; each one's place is its number in the list file.
export const SURVIVAL_TOPICS = [
  { id: 'first-aid', title: 'First aid' },
  { id: 'wilderness', title: 'Wilderness' },
  { id: 'disaster', title: 'Disasters' },
  { id: 'personal-safety', title: 'Personal safety' },
] as const;

export type SurvivalTopic = (typeof SURVIVAL_TOPICS)[number]['id'];

export type SurvivalGuide = {
  id: string;
  slot: number;
  title: string;
  topic: SurvivalTopic;
  text: string;
};

export const SURVIVAL_GUIDES: readonly SurvivalGuide[] = [
  {
    id: 'cpr',
    slot: 0,
    title: 'CPR',
    topic: 'first-aid',
    text: `Use when someone does not respond and is not breathing normally.

1. Check it is safe to go near.
2. Shout and tap their shoulders. No answer?
3. Call the emergency number (112, 911, 999) on speaker. Send someone for a defibrillator (AED).
4. Lay them on their back on a firm surface.
5. Heel of one hand in the centre of the chest, other hand on top, arms straight.
6. Push hard and fast: 5 to 6 cm deep, 100 to 120 a minute. Let the chest rise fully each time.
7. If trained: 30 pushes, then 2 rescue breaths, and repeat. If not: keep pushing without stopping.
8. AED here? Switch it on and do what it says.

Keep going until help takes over, they start breathing normally, or you cannot go on. Swap with someone every 2 minutes if you can.

Child: push one third of the chest's depth, with one hand if they are small. If trained, give 5 rescue breaths first.
`,
  },
  {
    id: 'bleeding',
    slot: 1,
    title: 'Severe bleeding',
    topic: 'first-aid',
    text: `1. Call the emergency number (112, 911, 999).
2. Protect your hands with gloves or a plastic bag if you can.
3. Press hard on the wound with a clean cloth, or your hand. Do not let go to look.
4. Keep pressing for at least 10 minutes.
5. Blood soaking through? Put more cloth on top and press harder. Do not take the first one off.
6. Deep wound: push cloth into it, then press.
7. Tie a bandage firmly over the pad once it stops.

Arm or leg still bleeding badly:
- Put a tourniquet 5 to 7 cm above the wound, not on a joint.
- A belt or strip of cloth with a stick to twist will do.
- Tighten until the bleeding stops. It will hurt.
- Write down the time. Never loosen it yourself.

Something stuck in the wound: leave it in and press around it.

Then lay them down, keep them warm, and watch for shock: pale, cold, sweaty skin and fast breathing.
`,
  },
  {
    id: 'choking',
    slot: 2,
    title: 'Choking',
    topic: 'first-aid',
    text: `Can they cough or speak? Tell them to keep coughing.

Cannot cough, speak or breathe:
1. Lean them forward and give up to 5 sharp blows between the shoulder blades with the heel of your hand.
2. Still stuck? Stand behind them, fist just above the navel, other hand over it. Pull sharply in and up, up to 5 times.
3. Keep swapping 5 back blows and 5 thrusts.
4. Call the emergency number (112, 911, 999) if it does not clear.

They stop responding: lay them down, call for help and start CPR.

Baby under 1: lay them face down along your forearm, head low, and give 5 back blows. Then turn them over and give 5 chest pushes with two fingers in the centre of the chest. Never squeeze a baby's belly.

Anyone given belly thrusts should see a doctor afterwards.

Alone? Thrust your belly against the back of a chair.
`,
  },
  {
    id: 'burns',
    slot: 3,
    title: 'Burns',
    topic: 'first-aid',
    text: `1. Stop the burning. Clothes on fire: stop, drop and roll.
2. Cool the burn under cool running water for 20 minutes. No ice.
3. Take off rings, watches and clothing near the burn, unless stuck to it.
4. Cover it loosely with cling film or a clean cloth that sheds no fluff.
5. Keep the rest of them warm.

Do not put on creams, butter or oil, and do not burst blisters.

Get medical help for a burn that is:
- bigger than their palm
- on the face, hands, feet, joints or groin
- deep, white or charred, even if painless
- from chemicals or electricity
- on a child or an older person

Chemical burn: brush off any powder, then rinse for at least 20 minutes.
Electrical burn: switch off the power before you touch them.
`,
  },
  {
    id: 'broken-bones',
    slot: 4,
    title: 'Broken bones',
    topic: 'first-aid',
    text: `Signs: pain, swelling, a strange shape, or they cannot move or bear weight on it.

1. Do not move them unless they are in danger.
2. After a fall or a blow to the head, think of the neck and spine: keep the head still and call for help.
3. Support the limb in the position you found it.
4. To move them, splint it: pad sticks, poles or a rolled mat along it, past the joint above and below the break, and tie them on firmly but not tight.
5. Check the fingers or toes stay warm and pink. Loosen the ties if not.
6. Bone through the skin: cover the wound with a clean cloth. Never push the bone back.
7. A cold pack wrapped in cloth, 20 minutes at a time, eases swelling.
8. Watch for shock: pale, cold, sweaty skin and fast breathing. Lay them down and keep them warm.

Do not give food or drink; they may need surgery.
`,
  },
  {
    id: 'stroke',
    slot: 5,
    title: 'Stroke',
    topic: 'first-aid',
    text: `Think FAST:

Face: does one side droop when they smile?
Arms: can they raise both arms and keep them up?
Speech: is it slurred or muddled?
Time: call the emergency number (112, 911, 999) at once.

Other signs: sudden weakness, numbness, blurred sight, confusion, a sudden very bad headache, or loss of balance.

While you wait:
- Note the time the signs began. Doctors need it.
- Keep them comfortable, sitting or lying with the head a little raised.
- Give nothing to eat or drink; they may not swallow safely.
- They stop responding but still breathe: roll them onto their side.
- They stop breathing normally: start CPR.

Signs that go away still need a doctor today.
`,
  },
  {
    id: 'allergic-reaction',
    slot: 6,
    title: 'Allergic reaction',
    topic: 'first-aid',
    text: `Severe reaction (anaphylaxis) signs:
- swollen lips, tongue, face or throat
- trouble breathing, wheezing, hoarse voice
- feeling faint, pale, floppy
- a rash or hives may also appear

1. Call the emergency number (112, 911, 999). Say anaphylaxis.
2. Use their adrenaline auto-injector now, into the outer thigh. It works through clothes. Hold it there as the label says.
3. Note the time.
4. Lay them down with legs raised. Struggling to breathe? Let them sit up instead.
5. Do not let them stand or walk, even if they feel better.
6. No better after 5 minutes? Give a second injector if there is one.
7. They stop breathing normally: start CPR.

Even after a good recovery they must go to hospital; it can come back.
`,
  },
  {
    id: 'hypothermia',
    slot: 7,
    title: 'Hypothermia',
    topic: 'wilderness',
    text: `Signs: shivering, cold pale skin, clumsy hands, slurred speech, confusion, drowsiness.
Getting worse: shivering stops, slow breathing, they stop responding.

1. Get out of the wind and wet. Find or make shelter.
2. Swap wet clothes for dry ones.
3. Put something thick between them and the ground.
4. Cover the head and neck. Wrap them in blankets, a sleeping bag or a survival blanket.
5. Share body heat inside a sleeping bag if you have nothing else.
6. Warm sweet drinks and food only if they are fully awake.
7. Warm the body's core first: chest, neck, armpits, groin.

Do not:
- rub their arms and legs
- give alcohol
- put them in hot water

Handle them gently. Severe signs: call the emergency number (112, 911, 999). Not breathing normally: start CPR.

Prevent it: stay dry, add layers before you get cold, eat and drink often.
`,
  },
  {
    id: 'heat-illness',
    slot: 8,
    title: 'Heat illness',
    topic: 'wilderness',
    text: `Heat exhaustion: heavy sweating, headache, dizziness, nausea, cramps, tiredness.
1. Move to shade or somewhere cool.
2. Lie down with legs raised. Loosen or take off extra clothing.
3. Sip water, or a sports or rehydration drink.
4. Cool the skin with wet cloths and fanning.
Better within 30 minutes? Rest for the day.

Heat stroke is an emergency: hot skin, confusion, strange behaviour, fits, collapse. They may stop sweating.
1. Call the emergency number (112, 911, 999).
2. Cool them as fast as you can: put them in cold water up to the neck if you can, or soak them with water and fan hard.
3. Cold packs on the neck, armpits and groin.
4. Keep going until help arrives or they feel normal.
5. Drinks only if fully awake.

Prevent it: drink before you are thirsty, rest in shade at midday, wear a hat and light clothes.
`,
  },
  {
    id: 'lost',
    slot: 9,
    title: 'Lost in the wild',
    topic: 'wilderness',
    text: `STOP:
Stop. Sit down and calm down.
Think. How did you get here? Who knows your plan?
Observe. Look for landmarks, trails, water, shelter, weather.
Plan. Decide what to do before you move.

Stay put if someone knows your route; searchers start there. Moving makes you harder to find.

Phone:
- Try the emergency number (112, 911, 999). It may work on any network.
- A text needs less signal than a call.
- Climb a little higher for signal.
- Save battery: airplane mode between tries, screen dim.

What you need, in order:
- Shelter: cold and wet kill in hours.
- Water: you can last about 3 days without it.
- Food: you can last weeks without it.

If you must move:
- Move in daylight only.
- Follow a trail or path, not a shortcut.
- Leave marks: arrows of sticks or stones.
- Streams can lead to cliffs and gorges; take care.
`,
  },
  {
    id: 'water',
    slot: 10,
    title: 'Finding water',
    topic: 'wilderness',
    text: `Where to look:
- Running streams and springs are best.
- Rain: catch it with a sheet or jacket.
- Dew: wipe it off grass at dawn with a cloth and wring it out.
- Snow and ice: melt it first. Eating it chills you.
- Green plants and insects flying low point to water.

Make it safe to drink:
- Boil it: a rolling boil for 1 minute, 3 minutes high in the mountains.
- Or use purifying tablets as the pack says.
- Or a proper water filter.
- No kit: fill a clear plastic bottle and leave it in full sun for 6 hours, or 2 days if cloudy.
- Cloudy water: let it settle, then strain through cloth first.

Never drink seawater or urine.

Save water by saving sweat: rest in shade in the heat of the day and move in the cool. Drink what you need rather than rationing it.
`,
  },
  {
    id: 'shelter-and-fire',
    slot: 11,
    title: 'Shelter and fire',
    topic: 'wilderness',
    text: `Shelter:
- Get out of the wind and rain first.
- The ground steals the most heat: lie on a thick layer of dry leaves, branches or a pack.
- Small is warm. Use a fallen tree, rock or bush as a wall.
- Lean branches against a log or ridge pole and pile leaves on thick.
- Avoid valley floors (cold air), dry riverbeds (floods), and dead trees or branches overhead.

Fire:
- Clear a spot down to bare earth, away from overhanging branches. Ring it with stones.
- Gather everything first:
  tinder: dry grass, birch bark, fluff, cotton wool
  kindling: twigs thin as a pencil
  fuel: sticks as thick as your wrist, then bigger
- Light the tinder, add kindling slowly, then fuel.
- Keep it small; it is easier to keep going.
- Never leave it alone. Drown it, stir it and drown it again before you go.
`,
  },
  {
    id: 'signalling',
    slot: 12,
    title: 'Signalling for help',
    topic: 'wilderness',
    text: `Phone first: call the emergency number (112, 911, 999). Give your location as map coordinates from your phone's maps app.

Three of anything means help:
- 3 whistle blasts, then wait a minute and repeat.
- 3 fires in a triangle.
- 3 flashes of a torch.

Torch or mirror SOS: 3 short, 3 long, 3 short.

By day:
- Flash a mirror, phone screen or anything shiny at aircraft.
- Spread bright clothing in the open.
- Write SOS big on the ground with rocks, logs or trampled snow.
- Green leaves on a fire make white smoke.

By night: a bright fire, or a torch.

To a helicopter: both arms raised in a Y means yes, we need help. One arm up and one down means no.

Stay near your signals once you have set them out.
`,
  },
  {
    id: 'earthquake',
    slot: 13,
    title: 'Earthquake',
    topic: 'disaster',
    text: `During the shaking:
- Drop to your hands and knees.
- Cover your head and neck, under a sturdy table if one is close.
- Hold on until the shaking stops.
- Stay away from windows, shelves and anything that can fall.
- In bed: stay there and cover your head with a pillow.
- Outside: move clear of buildings, trees, poles and power lines.
- Driving: pull over away from bridges and overpasses, and stay inside.
- Do not run outside during the shaking.

Afterwards:
- Expect aftershocks.
- Check yourself and others for injuries.
- Leave a damaged building. Use the stairs, never a lift.
- Smell gas? Open windows, get out, and use no flames or switches.
- Stay out of damaged buildings.

Near the coast: shaking that is strong or lasts a long time can bring a tsunami. Go to high ground or inland at once, without waiting for a warning.
`,
  },
  {
    id: 'protesting-safely',
    slot: 14,
    title: 'Protesting safely',
    topic: 'personal-safety',
    text: `Before you go:
- Go with a buddy. Agree where to meet if you are split up.
- Tell someone where you will be and when you expect to be back.
- Write a lawyer's or legal support number on your arm in marker.
- Bring water, snacks, your medicines, some cash and a paper map. Carry ID if the law where you are says to.
- Leave behind anything you would not want lost or searched.

Your phone:
- Safest is to leave it at home, or bring a spare phone with no accounts on it.
- If you take yours: use a long passcode and turn off face and fingerprint unlock, so no one can unlock it by holding it to you.
- Turn off location history. Airplane mode stops it talking to phone masts.
- Log out of apps you will not need.

Clothing:
- Plain clothes with no logos or slogans that single you out.
- Cover tattoos and distinctive marks. Tie back long hair.
- Comfortable closed shoes and layers.
- Glasses, not contact lenses, in case of tear gas.
- A face mask protects your health and privacy, where masks are legal.

While there:
- Know two ways out. Stay near the edges of a big crowd.
- Stay with your buddy and check on each other.
- Do not post photos that show other protesters' faces.
- Follow the marshals. Leave if it becomes unsafe.

Afterwards: leave together, check each other for injuries, and write down anything that happened to you.
`,
  },
  {
    id: 'nonviolent-action',
    slot: 15,
    title: 'Nonviolent action',
    topic: 'personal-safety',
    text: `Research on a century of campaigns found that nonviolent movements succeeded about twice as often as violent ones. They draw more people, and repression of peaceful people tends to backfire on those who order it.

Principles:
- A clear goal that people can agree on.
- Discipline: no violence, no damage and no insults, even when provoked.
- Numbers and variety: the more kinds of people take part, the stronger it is.
- Plan and train. Decide who does what before the day.

Ways to act:
Protest and persuasion:
- marches, rallies and vigils
- petitions, letters and public statements
- banners, art, music and symbols
Non-cooperation:
- strikes and walkouts
- boycotts of products, firms or events
- refusing to carry out unjust orders
Intervention:
- sit-ins and peaceful occupations
- blockades
- building your own alternatives: mutual aid, community services

Tips:
- Mix methods. Boycotts and stay-at-homes carry less risk than big crowds.
- Win over onlookers, workers, police and officials; do not make them enemies.
- Answer repression with calm, and record it.
- Give people roles: marshals, medics, legal observers, de-escalators, people who speak to the press.
`,
  },
  {
    id: 'de-escalation',
    slot: 16,
    title: 'De-escalating violence',
    topic: 'personal-safety',
    text: `Calming someone angry:
- Stay calm. Speak slowly and quietly.
- Keep a safe distance and your hands open and visible.
- Listen, and say back what you hear: "You're angry about..."
- Do not argue, threaten, point or touch.
- Offer a way out that saves face.
- Bring in a marshal or a calm friend if it is not working.

Agitators and provocateurs: people who push a peaceful crowd towards violence, to discredit it or for their own reasons. Watch for anyone who:
- urges others to throw things or break things
- brings weapons or objects to throw
- pushes a group towards police lines or away from the march
- does damage and then slips back into the crowd
Judge what people do, never how they look.

What to do:
- Do not follow calls to violence. Say clearly: "This is a peaceful protest."
- Tell the marshals or organisers and point the person out.
- Step away and make space around them, so it is clear they are not part of you. A crowd sitting down shows it is peaceful.
- Do not grab, fight or unmask anyone; it can turn violent and may be a crime.
- Record only if it is safe, and film what is done, not bystanders' faces.
- If violence starts, leave with your buddy.
`,
  },
  {
    id: 'crowds-and-tear-gas',
    slot: 17,
    title: 'Crowds and tear gas',
    topic: 'personal-safety',
    text: `In a big crowd:
- Stay near the edges and know where the exits are.
- Feel it getting too tight, or the crowd swaying? Move out early, diagonally across the flow.
- Never push against a crowd surge.
- Keep your arms up in front of your chest to protect your breathing.
- If you fall, get up at once. If you cannot, curl on your side and cover your head.
- Hemmed in by police? Stay calm, stay together, ask the marshals or legal observers what is happening.

Tear gas and pepper spray:
1. Move away, upwind and to higher ground if you can.
2. Do not rub your eyes or face.
3. Blink, and flush your eyes with plenty of clean water, from the inner corner outwards, head tilted.
4. Take out contact lenses.
5. Breathe slowly; it feels worse than it is.
6. Take off clothing that got sprayed and bag it.
7. Later, shower in cool water first, so it does not open your pores.

Get medical help for trouble breathing, an asthma attack, chest pain, or eyes still painful after an hour.

Hit by a projectile or hurt: see the first aid guides.
`,
  },
  {
    id: 'police-and-arrest',
    slot: 18,
    title: 'Police and arrest',
    topic: 'personal-safety',
    text: `Laws differ between countries and regions. Learn your rights where you live from a civil liberties group before you go.

With police:
- Stay calm, keep your hands visible and do not run.
- Never resist, even if it feels unfair. Complain later, through a lawyer.
- Ask: "Am I free to go?" If yes, walk away calmly.
- In many places you may stay silent and ask for a lawyer. Say: "I want to stay silent. I want a lawyer." Then do not explain or argue.
- Say: "I do not consent to a search." Do not physically stop one.
- Give your name or ID if the law requires it.
- Do not unlock your phone unless the law makes you.
- Remember names, badge numbers, times and places. Write them down as soon as you can.

If you are arrested:
- Call the legal support number on your arm.
- Say nothing about the protest to anyone but your lawyer, not even to people held with you.
- Ask for any medicine you need.

If you see an arrest:
- Film from a safe distance, where that is legal.
- Get the person's name and pass it to legal support.
`,
  },
];

// The guide that keeps a slot, if any.
export function guideInSlot(slot: number): SurvivalGuide | undefined {
  return SURVIVAL_GUIDES.find((guide) => guide.slot === slot);
}
